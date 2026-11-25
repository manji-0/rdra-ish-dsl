// Service of tests/codegen_conformance.rs: an in-memory store around the Rust that `codegen`
// prints for conformance.rdra, run through the generated harness. FAULT plants one bug:
//   skip-core        Pay writes the tag itself instead of calling the core
//   drop-row         Ship stores the order but not the shipment the core returns
//   accept-disabled  Cancel cancels from any state when the core refuses
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

impl Service {
    fn order(&self, id: OrderId, usecase: &'static str) -> Result<Order, Refused> {
        let row = self.orders.get(&id.0).cloned().ok_or(not_enabled(usecase))?;
        Ok(Order::from_row(row).expect("the store holds valid rows"))
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

    fn run_place(
        &mut self,
        parent_customer: CustomerId,
        order_id: OrderId,
        order_method: Method,
        order_note: String,
    ) -> Result<(), Refused> {
        let customer = self
            .customers
            .get(&parent_customer.0)
            .cloned()
            .ok_or(not_enabled("Place"))?;
        let r = place(customer, order_id, order_method, order_note);
        self.orders
            .insert(order_id.0, Order::Pending(r.order).into_row());
        Ok(())
    }

    fn run_pay(&mut self, order: OrderId, order_paid_at: DateTime) -> Result<(), Refused> {
        let state = self.order(order, "Pay")?;
        if self.fault == "skip-core" {
            let mut row = state.into_row();
            if row.status != OrderStatus::Pending {
                return Err(not_enabled("Pay"));
            }
            row.status = OrderStatus::Paid;
            self.orders.insert(order.0, row);
            return Ok(());
        }
        let PayOutcome::Ran(r) = run_pay(state, order_paid_at)?;
        self.orders.insert(order.0, Order::Paid(r.order).into_row());
        Ok(())
    }

    fn run_ship(&mut self, order: OrderId, shipment_id: ShipmentId) -> Result<(), Refused> {
        let ShipOutcome::Ran(r) = run_ship(self.order(order, "Ship")?, shipment_id)?;
        self.orders
            .insert(order.0, Order::Shipped(r.order).into_row());
        if self.fault != "drop-row" {
            self.shipments
                .insert(shipment_id.0, Shipment::Packed(r.shipment).into_row());
        }
        self.ev_shipped.push(r.ev_shipped);
        Ok(())
    }

    fn run_send(&mut self, shipment: ShipmentId, ev_shipped: EvShipped) -> Result<(), Refused> {
        if !self.ev_shipped.contains(&ev_shipped) {
            return Err(not_enabled("Send"));
        }
        let row = self
            .shipments
            .get(&shipment.0)
            .cloned()
            .ok_or(not_enabled("Send"))?;
        let state = Shipment::from_row(row).expect("the store holds valid rows");
        let SendOutcome::Ran(r) = run_send(state, ev_shipped)?;
        self.shipments
            .insert(shipment.0, Shipment::Sent(r.shipment).into_row());
        Ok(())
    }

    fn run_cancel(&mut self, order: OrderId) -> Result<(), Refused> {
        let state = self.order(order, "Cancel")?;
        match run_cancel(state.clone()) {
            Ok(CancelOutcome::FromPending(r)) => {
                self.orders
                    .insert(order.0, Order::Cancelled(r.order).into_row());
                Ok(())
            }
            Ok(CancelOutcome::FromPaid(r)) => {
                self.orders
                    .insert(order.0, Order::Cancelled(r.order).into_row());
                Ok(())
            }
            Err(Refused::NotEnabled(_)) if self.fault == "accept-disabled" => {
                let mut row = state.into_row();
                row.status = OrderStatus::Cancelled;
                self.orders.insert(order.0, row);
                Ok(())
            }
            Err(why) => Err(why),
        }
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
        Ok(r) => println!("ok {} traces, {} calls, {} refusals", r.traces, r.calls, r.refusals),
        Err(why) => {
            println!("FAIL {why}");
            std::process::exit(1);
        }
    }
}
