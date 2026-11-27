package svc

// The reference Go adapter of tests/codegen_adapter.rs and tests/codegen_code_check_go.rs: the
// reference service of service.go behind the black-box ModelAdapter that `codegen --adapter`
// prints for conformance.rdra (package modeladapter). The adapter knows the service's code; the
// harness knows only the model's plain data. The columns the model does not track (method,
// note, the time paid_at holds) get values of the adapter's choosing. With SPLIT=0 it is no
// SplitAdapter, so the harness runs the interleaved rows as plain calls.

import (
	"errors"
	"os"

	"example.com/shop/domain"
	"example.com/shop/modeladapter"
)

// Adapter is the reference service as a model adapter.
type Adapter struct {
	svc  *Service
	held func() error
}

// plain is an Adapter without Prepare and Commit.
type plain struct{ a *Adapter }

func (p plain) Reset()                                          { p.a.Reset() }
func (p plain) Seed(s modeladapter.ModelState)                  { p.a.Seed(s) }
func (p plain) Observe() modeladapter.ModelState                { return p.a.Observe() }
func (p plain) Invoke(c modeladapter.Call) modeladapter.Outcome { return p.a.Invoke(c) }

// NewAdapter returns the reference adapter, split unless SPLIT=0.
func NewAdapter() modeladapter.ModelAdapter {
	a := &Adapter{svc: New()}
	if os.Getenv("SPLIT") == "0" {
		return plain{a}
	}
	return a
}

// outcome is the outcome of a use case's error.
func outcome(err error) modeladapter.Outcome {
	var rejected *domain.Rejected
	var invalid *domain.InvalidArgument
	switch {
	case err == nil:
		return modeladapter.Ran
	case errors.As(err, &rejected):
		return modeladapter.Rejected
	case errors.As(err, &invalid):
		return modeladapter.InvalidArgument
	}
	return modeladapter.NotEnabled
}

// prepare runs the prepare of c and returns the commit of what it read.
func (a *Adapter) prepare(c modeladapter.Call) (func() error, error) {
	s := a.svc
	switch c := c.(type) {
	case modeladapter.PlaceCall:
		r, err := s.PreparePlace(domain.CustomerId(c.ParentCustomer), domain.OrderId(c.OrderId), domain.Method(c.OrderMethod), c.OrderNote)
		return func() error { return s.CommitPlace(r) }, err
	case modeladapter.PayCall:
		at, err := domain.ParseDateTime(c.OrderPaidAt)
		if err != nil {
			return nil, err
		}
		r, err := s.PreparePay(domain.OrderId(c.Order), at)
		return func() error { return s.CommitPay(r) }, err
	case modeladapter.ShipCall:
		r, err := s.PrepareShip(domain.OrderId(c.Order), domain.ShipmentId(c.ShipmentId))
		return func() error { return s.CommitShip(r) }, err
	case modeladapter.SendCall:
		r, err := s.PrepareSend(domain.ShipmentId(c.Shipment), domain.EvShipped{ShipmentId: domain.ShipmentId(c.EvShipped.ShipmentId)})
		return func() error { return s.CommitSend(r) }, err
	case modeladapter.CancelCall:
		r, err := s.PrepareCancel(domain.OrderId(c.Order))
		return func() error { return s.CommitCancel(r) }, err
	}
	return nil, &domain.NotEnabled{UseCase: c.Action()}
}

// Reset empties the store.
func (a *Adapter) Reset() {
	a.svc = New()
	a.held = nil
}

// Seed stores the model's rows, with a value of its own for each untracked column.
func (a *Adapter) Seed(s modeladapter.ModelState) {
	var snap domain.StoreSnapshot
	for _, c := range s.Customer {
		snap.Customer = append(snap.Customer, domain.Customer{Id: domain.CustomerId(c.Id)})
	}
	for _, o := range s.Order {
		row := domain.OrderRow{Id: domain.OrderId(o.Id), Status: domain.OrderStatus(o.Status), Method: domain.MethodCard, Qty: o.Qty, CustomerId: domain.CustomerId(o.CustomerId)}
		if o.PaidAt {
			at := domain.DateTime("2026-01-01T00:00:00Z")
			row.PaidAt = &at
		}
		snap.Order = append(snap.Order, row)
	}
	for _, r := range s.Shipment {
		snap.Shipment = append(snap.Shipment, domain.ShipmentRow{Id: domain.ShipmentId(r.Id), Status: domain.ShipmentStatus(r.Status), OrderId: domain.OrderId(r.OrderId)})
	}
	for _, e := range s.EvShipped {
		snap.EvShipped = append(snap.EvShipped, domain.EvShipped{ShipmentId: domain.ShipmentId(e.ShipmentId)})
	}
	if err := a.svc.Seed(snap); err != nil {
		panic(err)
	}
}

// Observe reads the store as the model sees it.
func (a *Adapter) Observe() modeladapter.ModelState {
	snap, err := a.svc.Snapshot()
	if err != nil {
		panic(err)
	}
	var s modeladapter.ModelState
	for _, c := range snap.Customer {
		s.Customer = append(s.Customer, modeladapter.CustomerRow{Id: int64(c.Id)})
	}
	for _, o := range snap.Order {
		s.Order = append(s.Order, modeladapter.OrderRow{Id: int64(o.Id), Status: modeladapter.OrderStatus(o.Status), PaidAt: o.PaidAt != nil, Qty: o.Qty, CustomerId: int64(o.CustomerId)})
	}
	for _, r := range snap.Shipment {
		s.Shipment = append(s.Shipment, modeladapter.ShipmentRow{Id: int64(r.Id), Status: modeladapter.ShipmentStatus(r.Status), OrderId: int64(r.OrderId)})
	}
	for _, e := range snap.EvShipped {
		s.EvShipped = append(s.EvShipped, modeladapter.EvShippedRaised{ShipmentId: int64(e.ShipmentId)})
	}
	return s
}

// Invoke prepares and commits c.
func (a *Adapter) Invoke(c modeladapter.Call) modeladapter.Outcome {
	commit, err := a.prepare(c)
	if err != nil {
		return outcome(err)
	}
	return outcome(commit())
}

// Prepare prepares c and holds its commit.
func (a *Adapter) Prepare(c modeladapter.Call) modeladapter.Outcome {
	commit, err := a.prepare(c)
	if err != nil {
		return outcome(err)
	}
	a.held = commit
	return modeladapter.Ran
}

// Commit commits what Prepare holds.
func (a *Adapter) Commit() modeladapter.Outcome {
	commit := a.held
	a.held = nil
	// FAULT=stub-commit: a commit that never stores, which only the split pass sees.
	if commit == nil || os.Getenv("FAULT") == "stub-commit" {
		return modeladapter.NotEnabled
	}
	return outcome(commit())
}
