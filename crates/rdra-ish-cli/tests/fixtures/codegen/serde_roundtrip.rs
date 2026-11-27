// The test codegen_code_build.rs runs against the code `[code.rust] serde = true` writes for
// conformance.rdra: a row, its state, and an event go through serde_json and back unchanged,
// in the model's wire spelling, and a value the parsers refuse is refused on the wire too.

use svc::domain::*;

fn row() -> OrderRow {
    OrderRow {
        id: OrderId::from(7),
        status: OrderStatus::Paid,
        paid_at: Some("2026-11-07T09:00:00+09:00".parse().unwrap()),
        method: Method::Card,
        qty: 2,
        note: "gift".to_string(),
        customer_id: CustomerId::from(3),
    }
}

#[test]
fn a_row_round_trips_in_its_wire_spelling() {
    let json = serde_json::to_value(row()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "id": 7,
            "status": "paid",
            "paid_at": "2026-11-07T09:00:00+09:00",
            "method": "card",
            "qty": 2,
            "note": "gift",
            "customer_id": 3,
        })
    );
    assert_eq!(serde_json::from_value::<OrderRow>(json).unwrap(), row());
}

#[test]
fn a_state_goes_over_the_wire_as_its_row() {
    let order = Order::from_row(row()).unwrap();
    let json = serde_json::to_value(&order).unwrap();
    assert_eq!(json, serde_json::to_value(row()).unwrap());
    assert_eq!(serde_json::from_value::<Order>(json).unwrap(), order);
}

#[test]
fn an_event_round_trips() {
    let event = EvShipped { shipment_id: ShipmentId::from(5) };
    let json = serde_json::to_string(&event).unwrap();
    assert_eq!(json, r#"{"shipment_id":5}"#);
    assert_eq!(serde_json::from_str::<EvShipped>(&json).unwrap(), event);
}

#[test]
fn the_wire_is_parsed_as_the_parsers_parse() {
    let mut json = serde_json::to_value(row()).unwrap();
    json["paid_at"] = "yesterday".into();
    assert!(serde_json::from_value::<OrderRow>(json).is_err(), "a DateTime the parser refuses");
    let mut json = serde_json::to_value(row()).unwrap();
    json["method"] = "Card".into();
    assert!(serde_json::from_value::<OrderRow>(json).is_err(), "a value set's Rust name");
    // A paid order keeps paid_at (the model's invariant): `from_row` refuses the row without it,
    // and so does the state's Deserialize.
    let mut json = serde_json::to_value(row()).unwrap();
    json["paid_at"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<OrderRow>(json.clone()).is_ok());
    assert!(serde_json::from_value::<Order>(json).is_err(), "a row that fits no state");
}
