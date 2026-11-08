// Driver of tests/codegen_behaviour.rs: runs the Go that `codegen` prints for order.rdra.
package domain

import (
	"errors"
	"reflect"
	"testing"
)

func row(status OrderStatus, paidAt *DateTime, qty int64) OrderRow {
	return OrderRow{Id: 1, Status: status, PaidAt: paidAt, Method: MethodCard, Qty: qty, Note: "n", CustomerId: 7}
}

func TestGenerated(t *testing.T) {
	pending := row(OrderStatusPending, nil, 1)
	order, err := OrderFromRow(pending)
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := order.(OrderPending); !ok {
		t.Fatalf("a pending row fits pending, got %T", order)
	}
	if !reflect.DeepEqual(OrderToRow(order), pending) {
		t.Fatal("round trip")
	}
	stamp := DateTime("t")
	var invalid *InvalidRow
	if _, err := OrderFromRow(row(OrderStatusPending, &stamp, 1)); !errors.As(err, &invalid) || invalid.Column != "paid_at" {
		t.Fatalf("pending with a payment time is invalid: %v", err)
	}
	if _, err := OrderFromRow(row(OrderStatusPaid, nil, 1)); !errors.As(err, &invalid) || invalid.Column != "paid_at" {
		t.Fatalf("paid without a payment time is invalid: %v", err)
	}

	out, err := RunPay(order, stamp)
	if err != nil || out.Ran == nil {
		t.Fatalf("Pay runs from pending: %v", err)
	}
	paid := out.Ran.Order
	if paid.PaidAt != stamp || paid.Qty != 2 {
		t.Fatalf("Pay writes: %+v", paid)
	}
	var notEnabled *NotEnabled
	if _, err := RunPay(paid, stamp); !errors.As(err, &notEnabled) {
		t.Fatalf("Pay does not run from paid: %v", err)
	}
	if c, err := RunCancel(paid); err != nil || c.FromPaid == nil {
		t.Fatalf("Cancel runs from paid: %v", err)
	}
	var rejected *Rejected
	if _, err := RunCancel(order); !errors.As(err, &rejected) || rejected.Condition != "Big" {
		t.Fatalf("Big rejects qty 1: %v", err)
	}

	if v := paid.RuleViolations(); len(v) != 0 {
		t.Fatalf("a paid order keeps its rules: %v", v)
	}
	paid.Qty = 5
	if v := Order(paid).RuleViolations(); !reflect.DeepEqual(v, []string{"Forbidden_Order_0"}) {
		t.Fatalf("qty 5 breaks Forbidden_Order_0: %v", v)
	}
}
