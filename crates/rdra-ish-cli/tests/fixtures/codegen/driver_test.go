// Driver of tests/codegen_behaviour.rs: runs the Go that `codegen` prints for order.rdra, from
// outside its package, as the code that finishes it does.
package domain_test

import (
	"errors"
	"reflect"
	"testing"

	. "example.com/domain"
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
	if _, ok := order.(OrderPending); !ok || order.Kind() != OrderKindPending {
		t.Fatalf("a pending row fits pending, got %T", order)
	}
	if !reflect.DeepEqual(OrderToRow(order), pending) {
		t.Fatal("round trip")
	}
	stamp, err := ParseDateTime("2026-01-01T09:00:00+09:00")
	if err != nil {
		t.Fatal(err)
	}
	var invalid *InvalidRow
	if _, err := OrderFromRow(row(OrderStatusPending, &stamp, 1)); !errors.As(err, &invalid) || invalid.Column != "paid_at" {
		t.Fatalf("pending with a payment time is invalid: %v", err)
	}
	if _, err := OrderFromRow(row(OrderStatusPaid, nil, 1)); !errors.As(err, &invalid) || invalid.Column != "paid_at" {
		t.Fatalf("paid without a payment time is invalid: %v", err)
	}
	bogus := row(OrderStatusPending, nil, 1)
	bogus.Method = Method("bogus")
	if _, err := OrderFromRow(bogus); !errors.As(err, &invalid) || invalid.Column != "method" {
		t.Fatalf("a value outside the value set is invalid: %v", err)
	}
	badStamp := DateTime("t")
	if _, err := OrderFromRow(row(OrderStatusPaid, &badStamp, 1)); !errors.As(err, &invalid) || invalid.Column != "paid_at" {
		t.Fatalf("a payment time not in the DateTime shape is invalid: %v", err)
	}
	if _, err := ParseOrderStatus("bogus"); err == nil {
		t.Fatal("ParseOrderStatus refuses text that is no value")
	}
	if v, err := ParseOrderStatus("paid"); err != nil || v != OrderStatusPaid {
		t.Fatalf("ParseOrderStatus reads a value: %v %v", v, err)
	}
	var invalidValue *InvalidValue
	for _, text := range []string{"t", "2026-01-01", "2026-01-01T09:00", "2026-01-01T09:00:00+0900"} {
		if _, err := ParseDateTime(text); !errors.As(err, &invalidValue) {
			t.Fatalf("ParseDateTime refuses %q: %v", text, err)
		}
	}
	for _, text := range []string{"2026-01-01T09:00Z", "2026-01-01T09:00:00.5Z", "2026-01-01T09:00:00-03:30"} {
		if _, err := ParseDateTime(text); err != nil {
			t.Fatalf("ParseDateTime reads %q: %v", text, err)
		}
	}
	for text, ok := range map[string]bool{"12": true, "-1.50": true, "1.": false, ".5": false, "1e3": false, "": false} {
		if _, err := ParseMoney(text); (err == nil) != ok {
			t.Fatalf("ParseMoney %q: %v", text, err)
		}
	}

	out, err := RunPay(order, stamp)
	if err != nil || out.Ran == nil {
		t.Fatalf("Pay runs from pending: %v", err)
	}
	paid := out.Ran.Order
	if paid.PaidAt() != stamp || paid.Qty() != 2 || paid.Kind() != OrderKindPaid {
		t.Fatalf("Pay writes: %+v", paid)
	}
	if order.Kind() != OrderKindPending {
		t.Fatal("Pay changes none of its arguments")
	}
	var notEnabled *NotEnabled
	var payErr *PayError
	if _, err := RunPay(paid, stamp); !errors.As(err, &notEnabled) || !errors.Is(err, ErrNotEnabled) || !errors.As(err, &payErr) || payErr.Kind != PayErrorNotEnabled {
		t.Fatalf("Pay does not run from paid: %v", err)
	}
	if _, err := RunPay(order, DateTime("t")); !errors.Is(err, ErrInvalidArgument) {
		t.Fatalf("Pay refuses a payment time not in the DateTime shape: %v", err)
	}
	if c, err := RunCancel(paid); err != nil || c.FromPaid == nil {
		t.Fatalf("Cancel runs from paid: %v", err)
	}
	var rejected *Rejected
	var cancelErr *CancelError
	if _, err := RunCancel(order); !errors.As(err, &rejected) || rejected.Condition != "Big" || !errors.Is(err, ErrRejected) || !errors.As(err, &cancelErr) || cancelErr.Kind != CancelErrorRejected {
		t.Fatalf("Big rejects qty 1: %v", err)
	}

	if v := paid.RuleViolations(); len(v) != 0 {
		t.Fatalf("a paid order keeps its rules: %v", v)
	}
	// A state's fields are unexported: a row with qty 5 is the way to such a state.
	bigRow := OrderToRow(paid)
	bigRow.Qty = 5
	big, err := OrderFromRow(bigRow)
	if err != nil {
		t.Fatal(err)
	}
	if v := big.RuleViolations(); !reflect.DeepEqual(v, []string{"Forbidden_Order_0"}) {
		t.Fatalf("qty 5 breaks Forbidden_Order_0: %v", v)
	}
	// A row and the state made from it share no value.
	*bigRow.PaidAt = DateTime("2027-01-01T00:00:00Z")
	if big.(OrderPaid).PaidAt() != stamp {
		t.Fatal("a state shares a value with the row it was made from")
	}

	// A nil state is no state: a function given one returns InvalidArgument, and does not panic.
	if _, err := Pay(nil, stamp); !errors.Is(err, ErrInvalidArgument) || !errors.As(err, &payErr) || payErr.Kind != PayErrorInvalidArgument {
		t.Fatalf("Pay refuses a nil state: %v", err)
	}
	if _, err := RunPay(nil, stamp); !errors.Is(err, ErrInvalidArgument) {
		t.Fatalf("RunPay refuses a nil state: %v", err)
	}
	if !reflect.DeepEqual(OrderToRow(nil), OrderRow{}) {
		t.Fatal("a nil state has the zero row")
	}
	// A value outside a value set is an argument the model never holds.
	if _, err := Place(Customer{Id: 7}, 2, Method("bogus"), "n"); !errors.Is(err, ErrInvalidArgument) {
		t.Fatalf("Place refuses a method outside Method: %v", err)
	}
}
