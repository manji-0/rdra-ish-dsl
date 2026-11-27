// The reference TypeScript adapter of tests/codegen_adapter.rs and tests/codegen_code_build_ts.rs:
// the reference service order_service.ts behind the black-box `ModelAdapter` that
// `codegen --adapter` prints for conformance.rdra (model.ts). The adapter knows the service's
// code; the harness knows only the model's plain data. Values cross by the core's parsers, and
// the columns the model does not track (`method`, `note`, the time `paid_at` holds) get values
// of the adapter's choosing. With SPLIT=0 it offers no prepare and commit, so the harness runs
// the interleaved rows as plain calls. Node runs it as it is (type stripping).
import * as d from "./domain.ts";
import * as m from "./model.ts";
import { OrderService } from "./order_service.ts";

declare const process: { env: Record<string, string | undefined> };

/** The model adapter the probe drives. */
export type Adapter = m.ModelAdapter;

/** A value of the core, by its parser: the model's values are the core's. */
function core<T>(r: d.Result<T, d.InvalidValue>): T {
  if (!r.ok) throw new globalThis.Error(`not a value of the core: ${r.error.reason}`);
  return r.value;
}

/** The refusal of a use case's error. */
function refusal(e: Readonly<{ kind: string }>): m.Outcome {
  switch (e.kind) {
    case "NotFound":
      return m.refused("notFound");
    case "Rejected":
      return m.refused("rejected");
    case "InvalidArgument":
      return m.refused("invalidArgument");
    case "StoreFailed":
      return m.refused("storeFailed");
    default:
      return m.refused("notEnabled");
  }
}

/** A prepared call: its commit, or its refusal. */
type Prepared = Readonly<{ ok: true; commit: () => Promise<m.Outcome> }> | Readonly<{ ok: false; outcome: m.Outcome }>;

/** The prepare of `call` on `svc`, with the commit of what it returns. */
async function prepareOn(svc: OrderService, call: m.Call): Promise<Prepared> {
  const done = <W>(
    r: d.Result<W, Readonly<{ kind: string }>>,
    commit: (w: W) => Promise<d.Result<void, Readonly<{ kind: string }>>>,
  ): Prepared =>
    r.ok
      ? {
          ok: true,
          commit: async () => {
            const committed = await commit(r.value);
            return committed.ok ? m.ran : refusal(committed.error);
          },
        }
      : { ok: false, outcome: refusal(r.error) };
  switch (call.kind) {
    case "Place":
      return done(
        await svc.preparePlace(
          core(d.CustomerId.parse(call.parent_customer)),
          core(d.OrderId.parse(call.order_id)),
          core(d.Method.parse(call.order_method)),
          call.order_note,
        ),
        svc.commitPlace,
      );
    case "Pay":
      return done(
        await svc.preparePay(core(d.OrderId.parse(call.order)), core(d.DateTime.parse(call.order_paid_at))),
        svc.commitPay,
      );
    case "Ship":
      return done(
        await svc.prepareShip(core(d.OrderId.parse(call.order)), core(d.ShipmentId.parse(call.shipment_id))),
        svc.commitShip,
      );
    case "Send":
      return done(
        await svc.prepareSend(core(d.ShipmentId.parse(call.shipment)), {
          kind: "EvShipped",
          shipment_id: core(d.ShipmentId.parse(call.ev_shipped.shipment_id)),
        }),
        svc.commitSend,
      );
    case "Cancel":
      return done(await svc.prepareCancel(core(d.OrderId.parse(call.order))), svc.commitCancel);
  }
}

/** The reference adapter: split unless SPLIT=0. */
export function adapter(): m.ModelAdapter {
  let svc = new OrderService();
  let held: (() => Promise<m.Outcome>) | undefined;
  const plain: m.ModelAdapter = {
    reset: () => {
      svc = new OrderService();
      held = undefined;
    },
    seed: (s) =>
      svc.seed({
        customer: s.customer.map((c) => ({ id: core(d.CustomerId.parse(c.id)) })),
        order: s.order.map((o) => ({
          id: core(d.OrderId.parse(o.id)),
          status: core(d.OrderStatus.parse(o.status)),
          paid_at: o.paid_at ? core(d.DateTime.parse("2026-01-01T00:00:00Z")) : null,
          method: "card",
          qty: o.qty,
          note: "",
          customer_id: core(d.CustomerId.parse(o.customer_id)),
        })),
        shipment: s.shipment.map((r) => ({
          id: core(d.ShipmentId.parse(r.id)),
          status: core(d.ShipmentStatus.parse(r.status)),
          order_id: core(d.OrderId.parse(r.order_id)),
        })),
        ev_shipped: s.ev_shipped.map((e) => ({ kind: "EvShipped", shipment_id: core(d.ShipmentId.parse(e.shipment_id)) })),
      }),
    observe: async () => {
      const s = await svc.snapshot();
      return {
        customer: s.customer.map((c) => ({ id: c.id })),
        order: s.order.map((o) => ({
          id: o.id,
          status: o.status,
          paid_at: o.paid_at !== null,
          qty: o.qty,
          customer_id: o.customer_id,
        })),
        shipment: s.shipment.map((r) => ({ id: r.id, status: r.status, order_id: r.order_id })),
        ev_shipped: s.ev_shipped.map((e) => ({ shipment_id: e.shipment_id })),
      };
    },
    invoke: async (call) => {
      const p = await prepareOn(svc, call);
      return p.ok ? p.commit() : p.outcome;
    },
  };
  if (process.env["SPLIT"] === "0") {
    return plain;
  }
  return {
    ...plain,
    prepare: async (call) => {
      const p = await prepareOn(svc, call);
      if (!p.ok) return p.outcome;
      held = p.commit;
      return m.ran;
    },
    commit: async () => {
      const commit = held;
      held = undefined;
      // FAULT=stub-commit: a commit that never stores, which only the split pass sees.
      return commit === undefined || process.env["FAULT"] === "stub-commit" ? m.refused("notEnabled") : commit();
    },
  };
}
