// Service of tests/codegen_conformance.rs: an in-memory store around the TypeScript that `codegen`
// prints for conformance.rdra, run through the generated harness. Each prepare reads and calls
// the core; each commit writes a row only where it still is what the result expects. FAULT
// plants one bug:
//   skip-core        Pay stores the tag itself instead of the row the core returns
//   drop-row         Ship stores the order but not the shipment the core returns
//   accept-disabled  Cancel cancels an order the core refuses, as if it were paid, unconditionally
//   unconditional    every commit writes without checking what the row still is
//   missing-not-enabled a call on an order that does not exist is NotEnabled, not NotFound
//   store-fails      Place's commit fails in the store where its row changed
import * as d from "./domain";

// What the service needs of Node, declared here: the repository installs no @types/node.
declare const require: (module: "node:fs") => { readFileSync: (path: string, encoding: "utf8") => string };
declare const process: { argv: string[]; env: Record<string, string | undefined>; exitCode: number | undefined };
const fs = require("node:fs");

const notEnabled = (usecase: string): d.Result<never, d.NotEnabled> => ({
  ok: false,
  error: { kind: "NotEnabled", usecase },
});
/** A call that names a row no row has: an HTTP service's 404. */
const notFound = (usecase: string, entity: string, id: number): d.Result<never, d.NotFound> => ({
  ok: false,
  error: { kind: "NotFound", usecase, entity, id: String(id) },
});
const done: d.Result<void, never> = { ok: true, value: undefined };

/** Whether a row, by its tag (undefined when absent), is still what a result expects. */
function holds(tag: string | undefined, expected: d.ExpectedSource): boolean {
  switch (expected.kind) {
    case "absent":
      return tag === undefined;
    case "present":
      return tag !== undefined;
    case "in":
      return expected.column === "status" && tag === expected.value;
    default:
      return d.assertNever(expected);
  }
}

/** A value the store holds: rows it wrote went through the core, so they parse. */
function stored<T>(r: d.Result<T, d.InvalidRow>): T {
  if (!r.ok) throw new Error(`the store holds an invalid row: ${JSON.stringify(r.error)}`);
  return r.value;
}

function service(fault: string): d.ConformancePort {
  let customers = new Map<number, d.Customer>();
  let orders = new Map<number, d.OrderRow>();
  let shipments = new Map<number, d.ShipmentRow>();
  let evShipped: d.EvShipped[] = [];

  /** The conditional write's WHERE clause, all of a commit's rows at once. */
  const check = (
    rowsOrders: readonly (readonly [d.OrderId, d.ExpectedSource])[],
    rowsShipments: readonly (readonly [d.ShipmentId, d.ExpectedSource])[],
  ): boolean =>
    fault === "unconditional" ||
    (rowsOrders.every(([id, e]) => holds(orders.get(id)?.status, e)) &&
      rowsShipments.every(([id, e]) => holds(shipments.get(id)?.status, e)));

  /** The refusal of a call on an order that does not exist. */
  const missing = (usecase: string, id: number): d.Result<never, d.NotEnabled | d.NotFound> =>
    fault === "missing-not-enabled" ? notEnabled(usecase) : notFound(usecase, "Order", id);

  const order = (id: d.OrderId): d.Order | undefined => {
    const row = orders.get(id);
    return row === undefined ? undefined : stored(d.Order.fromRow(row));
  };

  return {
    seed: async (rows) => {
      customers = new Map(rows.customer.map((c) => [c.id, c]));
      orders = new Map(rows.order.map((o) => [o.id, o]));
      shipments = new Map(rows.shipment.map((s) => [s.id, s]));
      evShipped = [...rows.ev_shipped];
    },
    snapshot: async () => ({
      customer: [...customers.values()],
      order: [...orders.values()],
      shipment: [...shipments.values()],
      ev_shipped: [...evShipped],
    }),

    preparePlace: async (parentCustomer, orderId, orderMethod, orderNote) => {
      const customer = customers.get(parentCustomer);
      return customer === undefined
        ? notFound("Place", "Customer", parentCustomer)
        : d.place(customer, orderId, orderMethod, orderNote);
    },
    commitPlace: async (r) => {
      if (!check([[r.order.id, r.order_expected]], [])) {
        return fault === "store-fails" ? { ok: false, error: { kind: "StoreFailed", usecase: "Place", cause: "the connection was lost" } } : notEnabled("Place");
      }
      orders.set(r.order.id, d.Order.toRow(r.order));
      return done;
    },

    preparePay: async (id, paidAt) => {
      const state = order(id);
      return state === undefined ? missing("Pay", id) : d.runPay(state, paidAt);
    },
    commitPay: async (r) => {
      if (!check([[r.order.id, r.order_expected]], [])) return notEnabled("Pay");
      const before = orders.get(r.order.id);
      if (fault === "skip-core" && before !== undefined) {
        orders.set(r.order.id, { ...before, status: "paid" });
        return done;
      }
      orders.set(r.order.id, d.Order.toRow(r.order));
      return done;
    },

    prepareShip: async (id, shipmentId) => {
      const state = order(id);
      return state === undefined ? missing("Ship", id) : d.runShip(state, shipmentId);
    },
    commitShip: async (r) => {
      if (!check([[r.order.id, r.order_expected]], [[r.shipment.id, r.shipment_expected]])) {
        return notEnabled("Ship");
      }
      orders.set(r.order.id, d.Order.toRow(r.order));
      if (fault !== "drop-row") {
        shipments.set(r.shipment.id, d.Shipment.toRow(r.shipment));
      }
      evShipped.push(r.ev_shipped);
      return done;
    },

    prepareSend: async (id, ev) => {
      if (!evShipped.some((e) => e.shipment_id === ev.shipment_id)) return notEnabled("Send");
      const row = shipments.get(id);
      return row === undefined ? notFound("Send", "Shipment", id) : d.runSend(stored(d.Shipment.fromRow(row)), ev);
    },
    commitSend: async (r) => {
      if (!check([], [[r.shipment.id, r.shipment_expected]])) return notEnabled("Send");
      shipments.set(r.shipment.id, d.Shipment.toRow(r.shipment));
      return done;
    },

    prepareCancel: async (id) => {
      const state = order(id);
      if (state === undefined) return missing("Cancel", id);
      const ran = d.runCancel(state);
      if (!ran.ok && ran.error.kind === "NotEnabled" && fault === "accept-disabled") {
        const row = { ...d.Order.toRow(state), status: "paid" as const };
        return d.runCancel(stored(d.Order.fromRow(row)));
      }
      return ran;
    },
    commitCancel: async (r) => {
      if (fault !== "accept-disabled" && !check([[r.order.id, r.order_expected]], [])) {
        return notEnabled("Cancel");
      }
      orders.set(r.order.id, d.Order.toRow(r.order));
      return done;
    },
  };
}

async function main(): Promise<void> {
  const path = process.argv[2];
  if (path === undefined) throw new Error("the traces file");
  const traces = fs.readFileSync(path, "utf8");
  const fault = process.env["FAULT"] ?? "";
  const r = await d.conform(() => service(fault), traces);
  if (r.ok) {
    const { traces: t, calls, refusals, interleavings } = r.value;
    console.log(`ok ${t} traces, ${calls} calls, ${refusals} refusals, ${interleavings} interleavings`);
  } else {
    console.log(`FAIL ${r.error}`);
    process.exitCode = 1;
  }
}

void main();
