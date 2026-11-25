// Service of tests/codegen_conformance.rs: an in-memory store around the Go that `codegen`
// prints for conformance.rdra, run through the generated harness from outside its package.
// FAULT plants one bug:
//
//	skip-core        Pay writes the tag itself instead of calling the core
//	drop-row         Ship stores the order but not the shipment the core returns
//	accept-disabled  Cancel cancels from any state when the core refuses
package domain_test

import (
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

func (s *service) order(id domain.OrderId, usecase string) (domain.Order, error) {
	row, ok := s.orders[id]
	if !ok {
		return nil, notEnabled(usecase)
	}
	state, err := domain.OrderFromRow(row)
	if err != nil {
		panic(err)
	}
	return state, nil
}

func (s *service) Seed(rows domain.StoreSnapshot) {
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
}

func (s *service) Snapshot() domain.StoreSnapshot {
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
	return snap
}

func (s *service) RunPlace(parent domain.CustomerId, id domain.OrderId, method domain.Method, note string) error {
	customer, ok := s.customers[parent]
	if !ok {
		return notEnabled("Place")
	}
	r := domain.Place(customer, id, method, note)
	s.orders[id] = domain.OrderToRow(r.Order)
	return nil
}

func (s *service) RunPay(id domain.OrderId, paidAt domain.DateTime) error {
	state, err := s.order(id, "Pay")
	if err != nil {
		return err
	}
	if s.fault == "skip-core" {
		row := domain.OrderToRow(state)
		if row.Status != domain.OrderStatusPending {
			return notEnabled("Pay")
		}
		row.Status = domain.OrderStatusPaid
		s.orders[id] = row
		return nil
	}
	out, err := domain.RunPay(state, paidAt)
	if err != nil {
		return err
	}
	s.orders[id] = domain.OrderToRow(out.Ran.Order)
	return nil
}

func (s *service) RunShip(id domain.OrderId, shipmentID domain.ShipmentId) error {
	state, err := s.order(id, "Ship")
	if err != nil {
		return err
	}
	out, err := domain.RunShip(state, shipmentID)
	if err != nil {
		return err
	}
	s.orders[id] = domain.OrderToRow(out.Ran.Order)
	if s.fault != "drop-row" {
		s.shipments[shipmentID] = domain.ShipmentToRow(out.Ran.Shipment)
	}
	s.evShipped = append(s.evShipped, out.Ran.EvShipped)
	return nil
}

func (s *service) RunSend(id domain.ShipmentId, ev domain.EvShipped) error {
	raised := false
	for _, e := range s.evShipped {
		raised = raised || e == ev
	}
	row, ok := s.shipments[id]
	if !raised || !ok {
		return notEnabled("Send")
	}
	state, err := domain.ShipmentFromRow(row)
	if err != nil {
		panic(err)
	}
	out, err := domain.RunSend(state, ev)
	if err != nil {
		return err
	}
	s.shipments[id] = domain.ShipmentToRow(out.Ran.Shipment)
	return nil
}

func (s *service) RunCancel(id domain.OrderId) error {
	state, err := s.order(id, "Cancel")
	if err != nil {
		return err
	}
	out, err := domain.RunCancel(state)
	if _, refused := err.(*domain.NotEnabled); refused && s.fault == "accept-disabled" {
		row := domain.OrderToRow(state)
		row.Status = domain.OrderStatusCancelled
		s.orders[id] = row
		return nil
	}
	if err != nil {
		return err
	}
	switch {
	case out.FromPending != nil:
		s.orders[id] = domain.OrderToRow(out.FromPending.Order)
	case out.FromPaid != nil:
		s.orders[id] = domain.OrderToRow(out.FromPaid.Order)
	}
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
	t.Logf("ok %d traces, %d calls, %d refusals", report.Traces, report.Calls, report.Refusals)
}
