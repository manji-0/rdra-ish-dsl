// Package svc is the reference Go service of tests/codegen_code_check_go.rs: an in-memory store
// around the Go that `codegen` prints for conformance.rdra, which implements every use case's
// contract. Each Prepare reads and calls the core; each Commit writes a row only where it still
// is what the result expects. The test plants one omission at a time by editing this file, and
// FAULT plants one bug the conformance run must catch:
//
//	skip-core        Pay stores the tag itself instead of the row the core returns
//	drop-row         Ship stores the order but not the shipment the core returns
//	accept-disabled  Cancel runs on an order the core refuses to cancel, as if it were paid
//	unconditional    every commit writes without checking what the row still is
package svc

import (
	"errors"
	"os"
	"sort"

	"example.com/shop/domain"
)

// Service is the service the contracts bind to.
type Service struct {
	fault     string
	customers map[domain.CustomerId]domain.Customer
	orders    map[domain.OrderId]domain.OrderRow
	shipments map[domain.ShipmentId]domain.ShipmentRow
	evShipped []domain.EvShipped
}

// New returns an empty service with the fault FAULT names, if any.
func New() *Service {
	return newService(os.Getenv("FAULT"))
}

func newService(fault string) *Service {
	return &Service{
		fault:     fault,
		customers: map[domain.CustomerId]domain.Customer{},
		orders:    map[domain.OrderId]domain.OrderRow{},
		shipments: map[domain.ShipmentId]domain.ShipmentRow{},
	}
}

func notEnabled(usecase string) error { return &domain.NotEnabled{UseCase: usecase} }

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

func (s *Service) orderHolds(id domain.OrderId, e domain.ExpectedSource) bool {
	row, ok := s.orders[id]
	return s.fault == "unconditional" || holds(string(row.Status), ok, e)
}

func (s *Service) shipmentHolds(id domain.ShipmentId, e domain.ExpectedSource) bool {
	row, ok := s.shipments[id]
	return s.fault == "unconditional" || holds(string(row.Status), ok, e)
}

func (s *Service) order(id domain.OrderId, usecase string) (domain.Order, error) {
	row, ok := s.orders[id]
	if !ok {
		return nil, notEnabled(usecase)
	}
	return domain.OrderFromRow(row)
}

// Seed empties the store and stores rows.
func (s *Service) Seed(rows domain.StoreSnapshot) {
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

// Snapshot returns what the store holds.
func (s *Service) Snapshot() domain.StoreSnapshot {
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

// PreparePlace reads what Place writes.
func (s *Service) PreparePlace(parent domain.CustomerId, id domain.OrderId, method domain.Method, note string) (domain.PlaceResult, error) {
	customer, ok := s.customers[parent]
	if !ok {
		return domain.PlaceResult{}, notEnabled("Place")
	}
	return domain.Place(customer, id, method, note)
}

// CommitPlace writes it.
func (s *Service) CommitPlace(r domain.PlaceResult) error {
	id := r.Order.Id()
	if !s.orderHolds(id, r.OrderExpected) {
		return notEnabled("Place")
	}
	s.orders[id] = domain.OrderToRow(r.Order)
	return nil
}

// PreparePay reads what Pay writes.
func (s *Service) PreparePay(id domain.OrderId, paidAt domain.DateTime) (domain.PayOutcome, error) {
	state, err := s.order(id, "Pay")
	if err != nil {
		return domain.PayOutcome{}, err
	}
	return domain.RunPay(state, paidAt)
}

// CommitPay writes it.
func (s *Service) CommitPay(out domain.PayOutcome) error {
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

// PrepareShip reads what Ship writes.
func (s *Service) PrepareShip(id domain.OrderId, shipmentID domain.ShipmentId) (domain.ShipOutcome, error) {
	state, err := s.order(id, "Ship")
	if err != nil {
		return domain.ShipOutcome{}, err
	}
	return domain.RunShip(state, shipmentID)
}

// CommitShip writes it.
func (s *Service) CommitShip(out domain.ShipOutcome) error {
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

// PrepareSend reads what Send writes.
func (s *Service) PrepareSend(id domain.ShipmentId, ev domain.EvShipped) (domain.SendOutcome, error) {
	raised := false
	for _, e := range s.evShipped {
		raised = raised || e == ev
	}
	row, ok := s.shipments[id]
	if !raised || !ok {
		return domain.SendOutcome{}, notEnabled("Send")
	}
	state, err := domain.ShipmentFromRow(row)
	if err != nil {
		return domain.SendOutcome{}, err
	}
	return domain.RunSend(state, ev)
}

// CommitSend writes it.
func (s *Service) CommitSend(out domain.SendOutcome) error {
	id := out.Ran.Shipment.Id()
	if !s.shipmentHolds(id, out.Ran.ShipmentExpected) {
		return notEnabled("Send")
	}
	s.shipments[id] = domain.ShipmentToRow(out.Ran.Shipment)
	return nil
}

// PrepareCancel reads what Cancel writes.
func (s *Service) PrepareCancel(id domain.OrderId) (domain.CancelOutcome, error) {
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
			return domain.CancelOutcome{}, err
		}
		return domain.RunCancel(paid)
	}
	return out, err
}

// CommitCancel writes it.
func (s *Service) CommitCancel(out domain.CancelOutcome) error {
	var order domain.OrderCancelled
	var expected domain.ExpectedSource
	switch {
	case out.FromPending != nil:
		order, expected = out.FromPending.Order, out.FromPending.OrderExpected
	case out.FromPaid != nil:
		order, expected = out.FromPaid.Order, out.FromPaid.OrderExpected
	}
	if order == nil {
		return notEnabled("Cancel")
	}
	id := order.Id()
	if s.fault != "accept-disabled" && !s.orderHolds(id, expected) {
		return notEnabled("Cancel")
	}
	s.orders[id] = domain.OrderToRow(order)
	return nil
}
