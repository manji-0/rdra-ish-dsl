// Service of tests/codegen_commit.rs: an in-memory store around the Rust that `codegen` prints
// for commit.rdra, whose every commit is one call of the generated `commit_writes`. The store
// implements `CommitStore` once, over every entity, run mark and event, so no commit names a
// result's fields. FAULT=keep-mark plants one bug: the store never deletes a run mark.
#[path = "domain.rs"]
#[allow(dead_code)] // the service uses part of the module
mod domain;
use domain::*;
use std::collections::BTreeMap;
use std::convert::Infallible;

/// Every table, the run marks by (use case, entity, row id), and the raised events.
#[derive(Clone, Default)]
struct Store {
    keep_marks: bool,
    accounts: BTreeMap<i64, Account>,
    orders: BTreeMap<i64, OrderRow>,
    shipments: BTreeMap<i64, ShipmentRow>,
    marks: BTreeMap<(&'static str, &'static str, i64), u8>,
    ev_shipped: Vec<EvShipped>,
}

impl Store {
    /// The tag of the row `id` names (`""` for a record), or `None` when there is no such row.
    fn tag(&self, id: &StoredId) -> Option<&'static str> {
        match id {
            StoredId::Account(id) => self.accounts.get(&id.get()).map(|_| ""),
            StoredId::Order(id) => self.orders.get(&id.get()).map(|r| r.status.as_str()),
            StoredId::Shipment(id) => self.shipments.get(&id.get()).map(|r| r.status.as_str()),
        }
    }

    /// Whether the row `id` names is still what `expected` names: the conditional write's
    /// WHERE clause.
    fn holds(&self, id: &StoredId, expected: ExpectedSource) -> bool {
        let tag = self.tag(id);
        match expected {
            ExpectedSource::Absent => tag.is_none(),
            ExpectedSource::Present => tag.is_some(),
            ExpectedSource::In { value, .. } => tag == Some(value),
        }
    }
}

impl CommitStore for Store {
    type Error = Infallible;

    fn write_row(&mut self, write: RowWrite) -> Result<bool, Infallible> {
        Ok(match write {
            RowWrite::Put { id, row, expected } => {
                let ok = self.holds(&id, expected);
                if ok {
                    match row {
                        StoredRow::Account(r) => drop(self.accounts.insert(r.id.get(), r)),
                        StoredRow::Order(r) => drop(self.orders.insert(r.id.get(), r)),
                        StoredRow::Shipment(r) => drop(self.shipments.insert(r.id.get(), r)),
                    }
                }
                ok
            }
            RowWrite::Delete { id, expected } => {
                let ok = self.holds(&id, expected);
                if ok {
                    match id {
                        StoredId::Account(id) => drop(self.accounts.remove(&id.get())),
                        StoredId::Order(id) => drop(self.orders.remove(&id.get())),
                        StoredId::Shipment(id) => drop(self.shipments.remove(&id.get())),
                    }
                }
                ok
            }
        })
    }

    fn write_mark(&mut self, write: MarkWrite) -> Result<bool, Infallible> {
        let (mark, delete) = match write {
            MarkWrite::Put(mark) => (mark, false),
            MarkWrite::Delete(mark) => (mark, true),
        };
        let id = match &mark {
            RunMark::ShipOrder { id, .. } => id.get(),
        };
        let key = (mark.usecase(), mark.entity(), id);
        let now = self.marks.get(&key).copied();
        let step = mark.step();
        if delete {
            if now != Some(step) {
                return Ok(false);
            }
            if !self.keep_marks {
                self.marks.remove(&key);
            }
        } else {
            let before = if step == 1 { None } else { Some(step - 1) };
            if now != before {
                return Ok(false);
            }
            self.marks.insert(key, step);
        }
        Ok(true)
    }

    fn record_event(&mut self, event: RaisedEvent) -> Result<(), Infallible> {
        match event {
            RaisedEvent::EvShipped(e) => self.ev_shipped.push(e),
        }
        Ok(())
    }
}

#[derive(Default)]
struct Service {
    store: Store,
}

impl Service {
    /// Commits any result in one transaction: the store as it was comes back on an error.
    fn commit<E: From<NotEnabled>>(&mut self, writes: impl IntoWrites) -> Result<(), E> {
        let saved = self.store.clone();
        commit_writes(&mut self.store, writes).map_err(|e| {
            self.store = saved;
            match e {
                CommitFailed::NotEnabled(e) => e.into(),
                CommitFailed::Store(never) => match never {},
            }
        })
    }

    fn order(&self, id: OrderId, usecase: &'static str) -> Result<Order, NotEnabled> {
        let row = self.store.orders.get(&id.get()).cloned();
        let row = row.ok_or(NotEnabled { usecase })?;
        Ok(Order::from_row(row).expect("the store holds valid rows"))
    }
}

impl ConformancePort for Service {
    fn seed(&mut self, rows: StoreSnapshot) {
        let keep_marks = self.store.keep_marks;
        self.store = Store {
            keep_marks,
            ..Store::default()
        };
        for a in rows.account {
            self.store.accounts.insert(a.id.get(), a);
        }
        for o in rows.order {
            self.store.orders.insert(o.id.get(), o);
        }
        for s in rows.shipment {
            self.store.shipments.insert(s.id.get(), s);
        }
        for m in rows.ship_order_runs {
            self.store.marks.insert(("Ship", "Order", m.id.get()), m.step);
        }
        self.store.ev_shipped = rows.ev_shipped;
    }

    fn snapshot(&mut self) -> StoreSnapshot {
        StoreSnapshot {
            account: self.store.accounts.values().cloned().collect(),
            order: self.store.orders.values().cloned().collect(),
            shipment: self.store.shipments.values().cloned().collect(),
            ship_order_runs: self
                .store
                .marks
                .iter()
                .map(|(&(_, _, id), &step)| ShipOrderRun {
                    id: OrderId::from(id),
                    step,
                })
                .collect(),
            ev_shipped: self.store.ev_shipped.clone(),
        }
    }
}

impl PlaceUseCase for Service {
    fn prepare_place(&mut self, parent_account: AccountId, order_id: OrderId) -> Result<PlaceResult, PlaceError> {
        let account = self.store.accounts.get(&parent_account.get()).cloned();
        place(account.ok_or(NotEnabled { usecase: "Place" })?, order_id)
    }

    fn commit_place(&mut self, write: PlaceResult) -> Result<(), PlaceError> {
        self.commit(write)
    }
}

impl PayUseCase for Service {
    fn prepare_pay(&mut self, order: OrderId) -> Result<PayOutcome, PayError> {
        run_pay(self.order(order, "Pay")?)
    }

    fn commit_pay(&mut self, write: PayOutcome) -> Result<(), PayError> {
        self.commit(write)
    }
}

impl ShipUseCase for Service {
    fn prepare_ship1(&mut self, order: OrderId) -> Result<Ship1Outcome, ShipError> {
        if self.store.marks.contains_key(&("Ship", "Order", order.get())) {
            return Err(NotEnabled { usecase: "Ship" }.into());
        }
        run_ship1(self.order(order, "Ship")?)
    }

    fn commit_ship1(&mut self, write: Ship1Outcome) -> Result<(), ShipError> {
        self.commit(write)
    }

    fn prepare_ship2(&mut self, order_mark: OrderId, shipment_id: ShipmentId) -> Result<Ship2Result, ShipError> {
        if self.store.marks.get(&("Ship", "Order", order_mark.get())) != Some(&1) {
            return Err(NotEnabled { usecase: "Ship" }.into());
        }
        ship2(ShipOrderMark1 { id: order_mark }, shipment_id)
    }

    fn commit_ship2(&mut self, write: Ship2Result) -> Result<(), ShipError> {
        self.commit(write)
    }
}

impl SendUseCase for Service {
    fn prepare_send(&mut self, shipment: ShipmentId, ev_shipped: EvShipped) -> Result<SendOutcome, SendError> {
        if !self.store.ev_shipped.contains(&ev_shipped) {
            return Err(NotEnabled { usecase: "Send" }.into());
        }
        let row = self.store.shipments.get(&shipment.get()).cloned();
        let row = row.ok_or(NotEnabled { usecase: "Send" })?;
        run_send(Shipment::from_row(row).expect("the store holds valid rows"), ev_shipped)
    }

    fn commit_send(&mut self, write: SendOutcome) -> Result<(), SendError> {
        self.commit(write)
    }
}

impl CancelUseCase for Service {
    fn prepare_cancel(&mut self, order: OrderId) -> Result<CancelOutcome, CancelError> {
        run_cancel(self.order(order, "Cancel")?)
    }

    fn commit_cancel(&mut self, write: CancelOutcome) -> Result<(), CancelError> {
        self.commit(write)
    }
}

impl PurgeUseCase for Service {
    fn prepare_purge(&mut self, order: OrderId) -> Result<PurgeOutcome, PurgeError> {
        let shipments: Vec<ShipmentRow> = self.store.shipments.values().cloned().collect();
        run_purge(self.order(order, "Purge")?, &shipments)
    }

    fn commit_purge(&mut self, write: PurgeOutcome) -> Result<(), PurgeError> {
        self.commit(write)
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("the traces file");
    let traces = std::fs::read_to_string(path).expect("reads the traces");
    let keep_marks = std::env::var("FAULT").is_ok_and(|f| f == "keep-mark");
    let new = || Service {
        store: Store {
            keep_marks,
            ..Store::default()
        },
    };
    match conform(new, &traces) {
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
