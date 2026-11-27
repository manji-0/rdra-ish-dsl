// Service of tests/codegen_commit.rs: an in-memory store around the Go that `codegen` prints
// for commit.rdra, run through the generated harness from outside its package. Every commit is
// one call of the generated CommitWrites: the store implements CommitStore once, over every
// entity, run mark and event, so no commit names a result's fields. An id is never used twice:
// a deleted row's id stays taken, as the model's slot does. FAULT=keep-mark plants one bug: the
// store never deletes a run mark.
package domain_test

import (
	"os"
	"sort"
	"testing"

	"example.com/domain"
)

type markKey struct {
	usecase, entity string
	id              domain.StoredId
}

type store struct {
	keepMarks bool
	accounts  map[domain.AccountId]domain.Account
	orders    map[domain.OrderId]domain.OrderRow
	shipments map[domain.ShipmentId]domain.ShipmentRow
	marks     map[markKey]int
	evShipped []domain.EvShipped
	used      map[domain.StoredId]bool
}

func newStore(keepMarks bool) *store {
	return &store{
		keepMarks: keepMarks,
		accounts:  map[domain.AccountId]domain.Account{},
		orders:    map[domain.OrderId]domain.OrderRow{},
		shipments: map[domain.ShipmentId]domain.ShipmentRow{},
		marks:     map[markKey]int{},
		used:      map[domain.StoredId]bool{},
	}
}

func (s *store) clone() *store {
	c := newStore(s.keepMarks)
	for k, v := range s.accounts {
		c.accounts[k] = v
	}
	for k, v := range s.orders {
		c.orders[k] = v
	}
	for k, v := range s.shipments {
		c.shipments[k] = v
	}
	for k, v := range s.marks {
		c.marks[k] = v
	}
	for k, v := range s.used {
		c.used[k] = v
	}
	c.evShipped = append(c.evShipped, s.evShipped...)
	return c
}

// tag is the tag of the row id names ("" for a record), and whether there is such a row.
func (s *store) tag(id domain.StoredId) (string, bool) {
	switch id := id.(type) {
	case domain.AccountId:
		_, ok := s.accounts[id]
		return "", ok
	case domain.OrderId:
		r, ok := s.orders[id]
		return string(r.Status), ok
	case domain.ShipmentId:
		r, ok := s.shipments[id]
		return string(r.Status), ok
	}
	return "", false
}

func (s *store) holds(id domain.StoredId, e domain.ExpectedSource) bool {
	tag, ok := s.tag(id)
	switch {
	case e.Absent:
		return !s.used[id]
	case e.Column == "":
		return ok
	default:
		return ok && tag == e.Value
	}
}

func (s *store) WriteRow(w domain.RowWrite) (bool, error) {
	if !s.holds(w.Id, w.Expected) {
		return false, nil
	}
	if w.Delete {
		switch id := w.Id.(type) {
		case domain.AccountId:
			delete(s.accounts, id)
		case domain.OrderId:
			delete(s.orders, id)
		case domain.ShipmentId:
			delete(s.shipments, id)
		}
		return true, nil
	}
	s.used[w.Id] = true
	switch r := w.Row.(type) {
	case domain.Account:
		s.accounts[r.Id] = r
	case domain.OrderRow:
		s.orders[r.Id] = r
	case domain.ShipmentRow:
		s.shipments[r.Id] = r
	}
	return true, nil
}

func (s *store) WriteMark(w domain.MarkWrite) (bool, error) {
	key := markKey{w.Mark.UseCase, w.Mark.Entity, w.Mark.Id}
	now, ok := s.marks[key]
	if w.Delete {
		if !ok || now != w.Mark.Step {
			return false, nil
		}
		if !s.keepMarks {
			delete(s.marks, key)
		}
		return true, nil
	}
	if (w.Mark.Step == 1 && ok) || (w.Mark.Step > 1 && (!ok || now != w.Mark.Step-1)) {
		return false, nil
	}
	s.marks[key] = w.Mark.Step
	return true, nil
}

func (s *store) RecordEvent(e domain.RaisedEvent) error {
	if e, ok := e.(domain.EvShipped); ok {
		s.evShipped = append(s.evShipped, e)
	}
	return nil
}

type service struct{ s *store }

// commit commits any result in one transaction: the store as it was comes back on an error.
func (sv *service) commit(w domain.Committable) error {
	saved := sv.s.clone()
	if err := domain.CommitWrites(sv.s, w); err != nil {
		sv.s = saved
		return err
	}
	return nil
}

func notEnabled(usecase string) error { return &domain.NotEnabled{UseCase: usecase} }

func (sv *service) order(id domain.OrderId, usecase string) (domain.Order, error) {
	row, ok := sv.s.orders[id]
	if !ok {
		return nil, notEnabled(usecase)
	}
	return domain.OrderFromRow(row)
}

func (sv *service) Seed(rows domain.StoreSnapshot) error {
	sv.s = newStore(sv.s.keepMarks)
	for _, a := range rows.Account {
		sv.s.accounts[a.Id] = a
		sv.s.used[a.Id] = true
	}
	for _, o := range rows.Order {
		sv.s.orders[o.Id] = o
		sv.s.used[o.Id] = true
	}
	for _, r := range rows.Shipment {
		sv.s.shipments[r.Id] = r
		sv.s.used[r.Id] = true
	}
	for _, m := range rows.ShipOrderRuns {
		sv.s.marks[markKey{"Ship", "Order", m.Id}] = m.Step
	}
	sv.s.evShipped = append([]domain.EvShipped(nil), rows.EvShipped...)
	return nil
}

func (sv *service) Snapshot() (domain.StoreSnapshot, error) {
	var snap domain.StoreSnapshot
	for _, a := range sv.s.accounts {
		snap.Account = append(snap.Account, a)
	}
	for _, o := range sv.s.orders {
		snap.Order = append(snap.Order, o)
	}
	for _, r := range sv.s.shipments {
		snap.Shipment = append(snap.Shipment, r)
	}
	for k, step := range sv.s.marks {
		snap.ShipOrderRuns = append(snap.ShipOrderRuns, domain.ShipOrderRun{Id: k.id.(domain.OrderId), Step: step})
	}
	sort.Slice(snap.Order, func(i, j int) bool { return snap.Order[i].Id < snap.Order[j].Id })
	sort.Slice(snap.Shipment, func(i, j int) bool { return snap.Shipment[i].Id < snap.Shipment[j].Id })
	snap.EvShipped = append(snap.EvShipped, sv.s.evShipped...)
	return snap, nil
}

func (sv *service) PreparePlace(parent domain.AccountId, orderId domain.OrderId) (domain.PlaceResult, error) {
	account, ok := sv.s.accounts[parent]
	if !ok {
		return domain.PlaceResult{}, notEnabled("Place")
	}
	return domain.Place(account, orderId)
}

func (sv *service) CommitPlace(w domain.PlaceResult) error { return sv.commit(w) }

func (sv *service) PreparePay(id domain.OrderId) (domain.PayOutcome, error) {
	o, err := sv.order(id, "Pay")
	if err != nil {
		return domain.PayOutcome{}, err
	}
	return domain.RunPay(o)
}

func (sv *service) CommitPay(w domain.PayOutcome) error { return sv.commit(w) }

func (sv *service) PrepareShip1(id domain.OrderId) (domain.Ship1Outcome, error) {
	if _, ok := sv.s.marks[markKey{"Ship", "Order", id}]; ok {
		return domain.Ship1Outcome{}, notEnabled("Ship")
	}
	o, err := sv.order(id, "Ship")
	if err != nil {
		return domain.Ship1Outcome{}, err
	}
	return domain.RunShip1(o)
}

func (sv *service) CommitShip1(w domain.Ship1Outcome) error { return sv.commit(w) }

func (sv *service) PrepareShip2(mark domain.OrderId, shipmentId domain.ShipmentId) (domain.Ship2Result, error) {
	if sv.s.marks[markKey{"Ship", "Order", mark}] != 1 {
		return domain.Ship2Result{}, notEnabled("Ship")
	}
	return domain.Ship2(domain.ShipOrderMark1{Id: mark}, shipmentId)
}

func (sv *service) CommitShip2(w domain.Ship2Result) error { return sv.commit(w) }

func (sv *service) PrepareSend(id domain.ShipmentId, ev domain.EvShipped) (domain.SendOutcome, error) {
	raised := false
	for _, e := range sv.s.evShipped {
		raised = raised || e == ev
	}
	row, ok := sv.s.shipments[id]
	if !raised || !ok {
		return domain.SendOutcome{}, notEnabled("Send")
	}
	state, err := domain.ShipmentFromRow(row)
	if err != nil {
		return domain.SendOutcome{}, err
	}
	return domain.RunSend(state, ev)
}

func (sv *service) CommitSend(w domain.SendOutcome) error { return sv.commit(w) }

func (sv *service) PrepareCancel(id domain.OrderId) (domain.CancelOutcome, error) {
	o, err := sv.order(id, "Cancel")
	if err != nil {
		return domain.CancelOutcome{}, err
	}
	return domain.RunCancel(o)
}

func (sv *service) CommitCancel(w domain.CancelOutcome) error { return sv.commit(w) }

func (sv *service) PreparePurge(id domain.OrderId) (domain.PurgeOutcome, error) {
	o, err := sv.order(id, "Purge")
	if err != nil {
		return domain.PurgeOutcome{}, err
	}
	var shipments []domain.ShipmentRow
	for _, r := range sv.s.shipments {
		shipments = append(shipments, r)
	}
	return domain.RunPurge(o, shipments)
}

func (sv *service) CommitPurge(w domain.PurgeOutcome) error { return sv.commit(w) }

func TestConformance(t *testing.T) {
	traces, err := os.ReadFile(os.Getenv("TRACES"))
	if err != nil {
		t.Fatal(err)
	}
	keepMarks := os.Getenv("FAULT") == "keep-mark"
	report, err := domain.Conform(func() domain.ConformancePort { return &service{newStore(keepMarks)} }, string(traces))
	if err != nil {
		t.Fatalf("FAIL %v", err)
	}
	t.Logf("ok %d traces, %d calls, %d refusals, %d interleavings", report.Traces, report.Calls, report.Refusals, report.Interleavings)
}
