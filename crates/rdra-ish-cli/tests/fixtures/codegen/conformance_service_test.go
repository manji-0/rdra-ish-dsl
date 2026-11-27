// Service of tests/codegen_conformance.rs: an in-memory store around the Go that `codegen`
// prints for conformance.rdra, run through the generated harness from outside its package.
// Each Prepare reads and calls the core; each Commit writes a row only where it still is what
// the result expects. FAULT plants one bug:
//
//	skip-core        Pay stores the tag itself instead of the row the core returns
//	drop-row         Ship stores the order but not the shipment the core returns
//	accept-disabled  Cancel cancels an order the core refuses, as if it were paid, unconditionally
//	unconditional    every commit writes without checking what the row still is
//	missing-not-enabled a call on an order that does not exist is NotEnabled, not NotFound
//	store-fails      Place's commit fails in the store where its row changed
package domain_test

import (
	"errors"
	"fmt"
	"os"
	"sort"
	"testing"

	"example.com/domain"
)

type service struct {
	fault     string
	customers map[domain.CustomerId]domain.Customer
	orders    map[domain.OrderId]domain.OrderRow
	shipments map[domain.ShipmentId]domain.ShipmentRow
	evShipped []domain.EvShipped
}

func newService(fault string) *service {
	return &service{
		fault:     fault,
		customers: map[domain.CustomerId]domain.Customer{},
		orders:    map[domain.OrderId]domain.OrderRow{},
		shipments: map[domain.ShipmentId]domain.ShipmentRow{},
	}
}

func notEnabled(usecase string) error { return &domain.NotEnabled{UseCase: usecase} }

// notFound is the error of a call that names a row no row has: an HTTP service's 404.
func notFound(usecase, entity string, id any) error {
	return &domain.NotFound{UseCase: usecase, Entity: entity, Id: fmt.Sprint(id)}
}

// holds tells whether a row, by its tag ("" and absent when there is none), is still what a
// result expects.
func holds(tag string, present bool, e domain.ExpectedSource) bool {
	switch {
	case e.Absent:
		return !present
	case e.Column == "":
		return present
	default:
		return present && e.Column == "status" && tag == e.Value
	}
}

func (s *service) orderHolds(id domain.OrderId, e domain.ExpectedSource) bool {
	row, ok := s.orders[id]
	return s.fault == "unconditional" || holds(string(row.Status), ok, e)
}

func (s *service) shipmentHolds(id domain.ShipmentId, e domain.ExpectedSource) bool {
	row, ok := s.shipments[id]
	return s.fault == "unconditional" || holds(string(row.Status), ok, e)
}

func (s *service) order(id domain.OrderId, usecase string) (domain.Order, error) {
	row, ok := s.orders[id]
	if !ok && s.fault == "missing-not-enabled" {
		return nil, notEnabled(usecase)
	}
	if !ok {
		return nil, notFound(usecase, "Order", id)
	}
	state, err := domain.OrderFromRow(row)
	if err != nil {
		panic(err)
	}
	return state, nil
}

func (s *service) Seed(rows domain.StoreSnapshot) error {
	*s = *newService(s.fault)
	for _, c := range rows.Customer {
		s.customers[c.Id] = c
	}
	for _, o := range rows.Order {
		s.orders[o.Id] = o
	}
	for _, sh := range rows.Shipment {
		s.shipments[sh.Id] = sh
	}
	s.evShipped = append(s.evShipped, rows.EvShipped...)
	return nil
}

func (s *service) Snapshot() (domain.StoreSnapshot, error) {
	var snap domain.StoreSnapshot
	for _, c := range s.customers {
		snap.Customer = append(snap.Customer, c)
	}
	for _, o := range s.orders {
		snap.Order = append(snap.Order, o)
	}
	for _, sh := range s.shipments {
		snap.Shipment = append(snap.Shipment, sh)
	}
	sort.Slice(snap.Order, func(i, j int) bool { return snap.Order[i].Id < snap.Order[j].Id })
	snap.EvShipped = append(snap.EvShipped, s.evShipped...)
	return snap, nil
}

func (s *service) PreparePlace(parent domain.CustomerId, id domain.OrderId, method domain.Method, note string) (domain.PlaceResult, error) {
	customer, ok := s.customers[parent]
	if !ok {
		return domain.PlaceResult{}, notFound("Place", "Customer", parent)
	}
	return domain.Place(customer, id, method, note)
}

func (s *service) CommitPlace(r domain.PlaceResult) error {
	id := r.Order.Id()
	if !s.orderHolds(id, r.OrderExpected) {
		if s.fault == "store-fails" {
			return &domain.StoreFailed{UseCase: "Place", Cause: errors.New("the connection was lost")}
		}
		return notEnabled("Place")
	}
	s.orders[id] = domain.OrderToRow(r.Order)
	return nil
}

func (s *service) PreparePay(id domain.OrderId, paidAt domain.DateTime) (domain.PayOutcome, error) {
	state, err := s.order(id, "Pay")
	if err != nil {
		return domain.PayOutcome{}, err
	}
	return domain.RunPay(state, paidAt)
}

func (s *service) CommitPay(out domain.PayOutcome) error {
	id := out.Ran.Order.Id()
	if !s.orderHolds(id, out.Ran.OrderExpected) {
		return notEnabled("Pay")
	}
	if s.fault == "skip-core" {
		row := s.orders[id]
		row.Status = domain.OrderStatusPaid
		s.orders[id] = row
		return nil
	}
	s.orders[id] = domain.OrderToRow(out.Ran.Order)
	return nil
}

func (s *service) PrepareShip(id domain.OrderId, shipmentID domain.ShipmentId) (domain.ShipOutcome, error) {
	state, err := s.order(id, "Ship")
	if err != nil {
		return domain.ShipOutcome{}, err
	}
	return domain.RunShip(state, shipmentID)
}

func (s *service) CommitShip(out domain.ShipOutcome) error {
	r := out.Ran
	order, shipment := r.Order.Id(), r.Shipment.Id()
	if !s.orderHolds(order, r.OrderExpected) || !s.shipmentHolds(shipment, r.ShipmentExpected) {
		return notEnabled("Ship")
	}
	s.orders[order] = domain.OrderToRow(r.Order)
	if s.fault != "drop-row" {
		s.shipments[shipment] = domain.ShipmentToRow(r.Shipment)
	}
	s.evShipped = append(s.evShipped, r.EvShipped)
	return nil
}

func (s *service) PrepareSend(id domain.ShipmentId, ev domain.EvShipped) (domain.SendOutcome, error) {
	raised := false
	for _, e := range s.evShipped {
		raised = raised || e == ev
	}
	row, ok := s.shipments[id]
	if !raised {
		return domain.SendOutcome{}, notEnabled("Send")
	}
	if !ok {
		return domain.SendOutcome{}, notFound("Send", "Shipment", id)
	}
	state, err := domain.ShipmentFromRow(row)
	if err != nil {
		panic(err)
	}
	return domain.RunSend(state, ev)
}

func (s *service) CommitSend(out domain.SendOutcome) error {
	id := out.Ran.Shipment.Id()
	if !s.shipmentHolds(id, out.Ran.ShipmentExpected) {
		return notEnabled("Send")
	}
	s.shipments[id] = domain.ShipmentToRow(out.Ran.Shipment)
	return nil
}

func (s *service) PrepareCancel(id domain.OrderId) (domain.CancelOutcome, error) {
	state, err := s.order(id, "Cancel")
	if err != nil {
		return domain.CancelOutcome{}, err
	}
	out, err := domain.RunCancel(state)
	if errors.Is(err, domain.ErrNotEnabled) && s.fault == "accept-disabled" {
		row := domain.OrderToRow(state)
		row.Status = domain.OrderStatusPaid
		paid, err := domain.OrderFromRow(row)
		if err != nil {
			panic(err)
		}
		return domain.RunCancel(paid)
	}
	return out, err
}

func (s *service) CommitCancel(out domain.CancelOutcome) error {
	var order domain.OrderCancelled
	var expected domain.ExpectedSource
	switch {
	case out.FromPending != nil:
		order, expected = out.FromPending.Order, out.FromPending.OrderExpected
	case out.FromPaid != nil:
		order, expected = out.FromPaid.Order, out.FromPaid.OrderExpected
	}
	id := order.Id()
	if s.fault != "accept-disabled" && !s.orderHolds(id, expected) {
		return notEnabled("Cancel")
	}
	s.orders[id] = domain.OrderToRow(order)
	return nil
}

func TestConformance(t *testing.T) {
	traces, err := os.ReadFile(os.Getenv("TRACES"))
	if err != nil {
		t.Fatal(err)
	}
	fault := os.Getenv("FAULT")
	report, err := domain.Conform(func() domain.ConformancePort { return newService(fault) }, string(traces))
	if err != nil {
		t.Fatalf("FAIL %v", err)
	}
	t.Logf("ok %d traces, %d calls, %d refusals, %d interleavings", report.Traces, report.Calls, report.Refusals, report.Interleavings)
}
