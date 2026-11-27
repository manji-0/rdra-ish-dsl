// Adapter of tests/codegen_adapter.rs and tests/codegen_code_build.rs: the reference service of
// conformance_service.rs behind the black-box `ModelAdapter` that `codegen --adapter` prints
// for conformance.rdra. The adapter knows the service's code; the harness knows only the
// model's plain data. Values cross by their text, and the columns the model does not track
// (`method`, `note`, the time `paid_at` holds) get values of the adapter's choosing. With
// SPLIT=0 it offers no prepare and commit, so the harness runs the interleaved rows as plain
// calls.
use crate::domain as core;
use crate::model as m;
use crate::service::Service;
use core::{CancelUseCase, ConformancePort, PayUseCase, PlaceUseCase, SendUseCase, ShipUseCase};

/// A prepared write of any use case.
enum Held {
    Place(core::PlaceResult),
    Pay(core::PayOutcome),
    Ship(core::ShipOutcome),
    Send(core::SendOutcome),
    Cancel(core::CancelOutcome),
}

pub struct Adapter {
    svc: Service,
    split: bool,
    held: Option<Held>,
}

/// The core's value of a model value, by its text.
fn text<T: std::str::FromStr>(t: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    t.parse().expect("the model's value is one of the core's")
}

fn place_refused(e: core::PlaceError) -> m::Refusal {
    match e {
        core::PlaceError::NotEnabled(_) => m::Refusal::NotEnabled,
    }
}

fn pay_refused(e: core::PayError) -> m::Refusal {
    match e {
        core::PayError::NotEnabled(_) => m::Refusal::NotEnabled,
    }
}

fn ship_refused(e: core::ShipError) -> m::Refusal {
    match e {
        core::ShipError::NotEnabled(_) => m::Refusal::NotEnabled,
    }
}

fn send_refused(e: core::SendError) -> m::Refusal {
    match e {
        core::SendError::NotEnabled(_) => m::Refusal::NotEnabled,
        core::SendError::InvalidArgument(_) => m::Refusal::InvalidArgument,
    }
}

fn cancel_refused(e: core::CancelError) -> m::Refusal {
    match e {
        core::CancelError::NotEnabled(_) => m::Refusal::NotEnabled,
        core::CancelError::Rejected(_) => m::Refusal::Rejected,
    }
}

impl Default for Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl Adapter {
    /// The reference service, with the fault `FAULT` names; split unless SPLIT=0.
    pub fn new() -> Self {
        Adapter {
            svc: Service::new(),
            split: std::env::var("SPLIT").as_deref() != Ok("0"),
            held: None,
        }
    }

    fn prepare_call(&mut self, call: &m::Call) -> Result<Held, m::Refusal> {
        Ok(match call {
            m::Call::Place {
                parent_customer,
                order_id,
                order_method,
                order_note,
            } => Held::Place(
                self.svc
                    .prepare_place(
                        (*parent_customer).into(),
                        (*order_id).into(),
                        text(order_method.as_str()),
                        order_note.clone(),
                    )
                    .map_err(place_refused)?,
            ),
            m::Call::Pay {
                order,
                order_paid_at,
            } => Held::Pay(
                self.svc
                    .prepare_pay((*order).into(), text(order_paid_at))
                    .map_err(pay_refused)?,
            ),
            m::Call::Ship { order, shipment_id } => Held::Ship(
                self.svc
                    .prepare_ship((*order).into(), (*shipment_id).into())
                    .map_err(ship_refused)?,
            ),
            m::Call::Send {
                shipment,
                ev_shipped,
            } => Held::Send(
                self.svc
                    .prepare_send(
                        (*shipment).into(),
                        core::EvShipped {
                            shipment_id: ev_shipped.shipment_id.into(),
                        },
                    )
                    .map_err(send_refused)?,
            ),
            m::Call::Cancel { order } => Held::Cancel(
                self.svc
                    .prepare_cancel((*order).into())
                    .map_err(cancel_refused)?,
            ),
        })
    }

    fn commit_held(&mut self, held: Held) -> Result<(), m::Refusal> {
        match held {
            Held::Place(w) => self.svc.commit_place(w).map_err(place_refused),
            Held::Pay(w) => self.svc.commit_pay(w).map_err(pay_refused),
            Held::Ship(w) => self.svc.commit_ship(w).map_err(ship_refused),
            Held::Send(w) => self.svc.commit_send(w).map_err(send_refused),
            Held::Cancel(w) => self.svc.commit_cancel(w).map_err(cancel_refused),
        }
    }
}

impl m::ModelAdapter for Adapter {
    fn reset(&mut self) {
        self.svc = Service::new();
        self.held = None;
    }

    fn seed(&mut self, s: &m::ModelState) {
        self.svc.seed(core::StoreSnapshot {
            customer: s
                .customer
                .iter()
                .map(|c| core::Customer { id: c.id.into() })
                .collect(),
            order: s
                .order
                .iter()
                .map(|o| core::OrderRow {
                    id: o.id.into(),
                    status: text(o.status.as_str()),
                    paid_at: o.paid_at.then(|| text("2026-01-01T00:00:00Z")),
                    method: core::Method::Card,
                    qty: o.qty,
                    note: String::new(),
                    customer_id: o.customer_id.into(),
                })
                .collect(),
            shipment: s
                .shipment
                .iter()
                .map(|r| core::ShipmentRow {
                    id: r.id.into(),
                    status: text(r.status.as_str()),
                    order_id: r.order_id.into(),
                })
                .collect(),
            ev_shipped: s
                .ev_shipped
                .iter()
                .map(|e| core::EvShipped {
                    shipment_id: e.shipment_id.into(),
                })
                .collect(),
        });
    }

    fn observe(&mut self) -> m::ModelState {
        let s = self.svc.snapshot();
        m::ModelState {
            customer: s
                .customer
                .iter()
                .map(|c| m::CustomerRow { id: c.id.get() })
                .collect(),
            order: s
                .order
                .iter()
                .map(|o| m::OrderRow {
                    id: o.id.get(),
                    status: text(o.status.as_str()),
                    paid_at: o.paid_at.is_some(),
                    qty: o.qty,
                    customer_id: o.customer_id.get(),
                })
                .collect(),
            shipment: s
                .shipment
                .iter()
                .map(|r| m::ShipmentRow {
                    id: r.id.get(),
                    status: text(r.status.as_str()),
                    order_id: r.order_id.get(),
                })
                .collect(),
            ev_shipped: s
                .ev_shipped
                .iter()
                .map(|e| m::EvShippedRaised {
                    shipment_id: e.shipment_id.get(),
                })
                .collect(),
        }
    }

    fn invoke(&mut self, call: &m::Call) -> Result<(), m::Refusal> {
        let held = self.prepare_call(call)?;
        self.commit_held(held)
    }

    fn split(&mut self) -> Option<&mut dyn m::SplitAdapter> {
        if self.split {
            Some(self)
        } else {
            None
        }
    }
}

impl m::SplitAdapter for Adapter {
    fn prepare(&mut self, call: &m::Call) -> Result<(), m::Refusal> {
        self.prepare_call(call).map(|h| self.held = Some(h))
    }

    fn commit(&mut self) -> Result<(), m::Refusal> {
        match self.held.take() {
            Some(h) => self.commit_held(h),
            None => Err(m::Refusal::NotEnabled),
        }
    }
}
