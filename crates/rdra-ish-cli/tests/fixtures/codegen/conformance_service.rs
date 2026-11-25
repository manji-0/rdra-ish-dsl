// Service of tests/codegen_conformance.rs: an in-memory store around the Rust that `codegen`
// prints for conformance.rdra, run through the generated harness. Each prepare reads and calls
// the core; each commit writes a row only where it still is what the result expects. FAULT
// plants one bug:
//   skip-core        Pay stores the tag itself instead of the row the core returns
//   drop-row         Ship stores the order but not the shipment the core returns
//   accept-disabled  Cancel cancels an order the core refuses, as if it were paid, unconditionally
//   unconditional    every commit writes without checking what the row still is
#[path = "domain.rs"]
mod domain;
use domain::*;
use std::collections::BTreeMap;

#[derive(Default)]
struct Service {
    fault: String,
    customers: BTreeMap<i64, Customer>,
    orders: BTreeMap<i64, OrderRow>,
    shipments: BTreeMap<i64, ShipmentRow>,
    ev_shipped: Vec<EvShipped>,
}

fn not_enabled(usecase: &'static str) -> Refused {
    Refused::NotEnabled(NotEnabled { usecase })
}

/// Whether a row, by its tag (`None` when absent), is still what a result expects.
fn holds(tag: Option<&str>, expected: ExpectedSource) -> bool {
    match expected {
        ExpectedSource::Absent => tag.is_none(),
        ExpectedSource::Present => tag.is_some(),
        ExpectedSource::In { column, value } => column == "status" && tag == Some(value),
    }
}

impl Service {
    fn order(&self, id: OrderId, usecase: &'static str) -> Result<Order, Refused> {
        let row = self.orders.get(&id.0).cloned().ok_or(not_enabled(usecase))?;
        Ok(Order::from_row(row).expect("the store holds valid rows"))
    }

    /// The conditional write's WHERE clause, all of a commit's rows at once: a commit whose
    /// rows another call changed writes nothing and is NotEnabled.
    fn check(
        &self,
        usecase: &'static str,
        orders: &[(OrderId, ExpectedSource)],
        shipments: &[(ShipmentId, ExpectedSource)],
    ) -> Result<(), Refused> {
        if self.fault == "unconditional" {
            return Ok(());
        }
        let order_ok = orders.iter().all(|(id, e)| {
            holds(self.orders.get(&id.0).map(|r| r.status.as_str()), *e)
        });
        let shipment_ok = shipments.iter().all(|(id, e)| {
            holds(self.shipments.get(&id.0).map(|r| r.status.as_str()), *e)
        });
        if order_ok && shipment_ok {
            Ok(())
        } else {
            Err(not_enabled(usecase))
        }
    }
}

impl ConformancePort for Service {
    fn seed(&mut self, rows: StoreSnapshot) {
        *self = Service {
            fault: std::mem::take(&mut self.fault),
            ..Service::default()
        };
        for c in rows.customer {
            self.customers.insert(c.id.0, c);
        }
        for o in rows.order {
            self.orders.insert(o.id.0, o);
        }
        for s in rows.shipment {
            self.shipments.insert(s.id.0, s);
        }
        self.ev_shipped = rows.ev_shipped;
    }

    fn snapshot(&mut self) -> StoreSnapshot {
        StoreSnapshot {
            customer: self.customers.values().cloned().collect(),
            order: self.orders.values().cloned().collect(),
            shipment: self.shipments.values().cloned().collect(),
            ev_shipped: self.ev_shipped.clone(),
        }
    }

    fn prepare_place(
        &mut self,
        parent_customer: CustomerId,
        order_id: OrderId,
        order_method: Method,
        order_note: String,
    ) -> Result<PlaceResult, Refused> {
        let customer = self
            .customers
            .get(&parent_customer.0)
            .cloned()
            .ok_or(not_enabled("Place"))?;
        Ok(place(customer, order_id, order_method, order_note))
    }

    fn commit_place(&mut self, r: PlaceResult) -> Result<(), Refused> {
        let id = *r.order.id();
        self.check("Place", &[(id, r.order_expected)], &[])?;
        self.orders.insert(id.0, Order::Pending(r.order).into_row());
        Ok(())
    }

    fn prepare_pay(&mut self, order: OrderId, order_paid_at: DateTime) -> Result<PayOutcome, Refused> {
        run_pay(self.order(order, "Pay")?, order_paid_at)
    }

    fn commit_pay(&mut self, write: PayOutcome) -> Result<(), Refused> {
        let PayOutcome::Ran(r) = write;
        let id = *r.order.id();
        self.check("Pay", &[(id, r.order_expected)], &[])?;
        if self.fault == "skip-core" {
            let mut row = self.orders[&id.0].clone();
            row.status = OrderStatus::Paid;
            self.orders.insert(id.0, row);
            return Ok(());
        }
        self.orders.insert(id.0, Order::Paid(r.order).into_row());
        Ok(())
    }

    fn prepare_ship(&mut self, order: OrderId, shipment_id: ShipmentId) -> Result<ShipOutcome, Refused> {
        run_ship(self.order(order, "Ship")?, shipment_id)
    }

    fn commit_ship(&mut self, write: ShipOutcome) -> Result<(), Refused> {
        let ShipOutcome::Ran(r) = write;
        let (order, shipment) = (*r.order.id(), *r.shipment.id());
        self.check(
            "Ship",
            &[(order, r.order_expected)],
            &[(shipment, r.shipment_expected)],
        )?;
        self.orders.insert(order.0, Order::Shipped(r.order).into_row());
        if self.fault != "drop-row" {
            self.shipments
                .insert(shipment.0, Shipment::Packed(r.shipment).into_row());
        }
        self.ev_shipped.push(r.ev_shipped);
        Ok(())
    }

    fn prepare_send(&mut self, shipment: ShipmentId, ev_shipped: EvShipped) -> Result<SendOutcome, Refused> {
        if !self.ev_shipped.contains(&ev_shipped) {
            return Err(not_enabled("Send"));
        }
        let row = self
            .shipments
            .get(&shipment.0)
            .cloned()
            .ok_or(not_enabled("Send"))?;
        run_send(
            Shipment::from_row(row).expect("the store holds valid rows"),
            ev_shipped,
        )
    }

    fn commit_send(&mut self, write: SendOutcome) -> Result<(), Refused> {
        let SendOutcome::Ran(r) = write;
        let id = *r.shipment.id();
        self.check("Send", &[], &[(id, r.shipment_expected)])?;
        self.shipments
            .insert(id.0, Shipment::Sent(r.shipment).into_row());
        Ok(())
    }

    fn prepare_cancel(&mut self, order: OrderId) -> Result<CancelOutcome, Refused> {
        let state = self.order(order, "Cancel")?;
        match run_cancel(state.clone()) {
            Err(Refused::NotEnabled(_)) if self.fault == "accept-disabled" => {
                let row = OrderRow {
                    status: OrderStatus::Paid,
                    ..state.into_row()
                };
                run_cancel(Order::from_row(row).expect("a paid row"))
            }
            other => other,
        }
    }

    fn commit_cancel(&mut self, write: CancelOutcome) -> Result<(), Refused> {
        let (r, expected) = match write {
            CancelOutcome::FromPending(r) => (r.order, r.order_expected),
            CancelOutcome::FromPaid(r) => (r.order, r.order_expected),
        };
        let id = *r.id();
        if self.fault != "accept-disabled" {
            self.check("Cancel", &[(id, expected)], &[])?;
        }
        self.orders.insert(id.0, Order::Cancelled(r).into_row());
        Ok(())
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("the traces file");
    let traces = std::fs::read_to_string(path).expect("reads the traces");
    let fault = std::env::var("FAULT").unwrap_or_default();
    match conform(
        || Service {
            fault: fault.clone(),
            ..Service::default()
        },
        &traces,
    ) {
        Ok(r) => println!(
            "ok {} traces, {} calls, {} refusals, {} interleavings",
            r.traces, r.calls, r.refusals, r.interleavings
        ),
        Err(why) => {
            println!("FAIL {why}");
            std::process::exit(1);
        }
    }
}
