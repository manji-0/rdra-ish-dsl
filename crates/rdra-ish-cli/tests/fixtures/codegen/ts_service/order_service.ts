// The reference TypeScript service of tests/codegen_code_check.rs: an in-memory store around the
// TypeScript that `codegen` prints for conformance.rdra, bound by `[code.ts]` in rdra.toml. The
// test plants one omission at a time in it and expects one named diagnostic from
// `check --code=build`; FAULT (in the environment) plants a bug only `check --code=run` sees:
//   skip-core        Pay stores the tag itself instead of the row the core returns
//   drop-row         Ship stores the order but not the shipment the core returns
//   accept-disabled  Cancel cancels an order the core refuses, as if it were paid, unconditionally
//   unconditional    every commit writes without checking what the row still is
//   ignore-id        a call on an order or a shipment runs on the first one, whatever its id
//   constant-observe the snapshot is what was seeded, whatever the calls stored
// Node runs it as it is (type stripping): the imports name the `.ts` files.
import * as d from "./domain.ts";

// What the service needs of Node, declared here: the repository installs no @types/node.
declare const process: { env: Record<string, string | undefined> };

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
  if (!r.ok) throw new globalThis.Error(`the store holds an invalid row: ${r.error.reason}`);
  return r.value;
}

export class OrderService {
  private readonly fault = process.env["FAULT"] ?? "";
  private customers = new Map<number, d.Customer>();
  private orders = new Map<number, d.OrderRow>();
  private shipments = new Map<number, d.ShipmentRow>();
  private evShipped: d.EvShipped[] = [];
  private seeded: d.StoreSnapshot | undefined;

  /** The conditional write's WHERE clause, all of a commit's rows at once. */
  private check(
    rowsOrders: readonly (readonly [d.OrderId, d.ExpectedSource])[],
    rowsShipments: readonly (readonly [d.ShipmentId, d.ExpectedSource])[],
  ): boolean {
    return (
      this.fault === "unconditional" ||
      (rowsOrders.every(([id, e]) => holds(this.orders.get(id)?.status, e)) &&
        rowsShipments.every(([id, e]) => holds(this.shipments.get(id)?.status, e)))
    );
  }

  private order(id: d.OrderId): d.Order | undefined {
    const row = this.fault === "ignore-id" ? [...this.orders.values()][0] : this.orders.get(id);
    return row === undefined ? undefined : stored(d.Order.fromRow(row));
  }

  seed = async (rows: d.StoreSnapshot): Promise<void> => {
    this.seeded = rows;
    this.customers = new Map(rows.customer.map((c) => [c.id, c]));
    this.orders = new Map(rows.order.map((o) => [o.id, o]));
    this.shipments = new Map(rows.shipment.map((s) => [s.id, s]));
    this.evShipped = [...rows.ev_shipped];
  };

  snapshot = async (): Promise<d.StoreSnapshot> =>
    this.fault === "constant-observe" && this.seeded !== undefined
      ? this.seeded
      : {
          customer: [...this.customers.values()],
          order: [...this.orders.values()],
          shipment: [...this.shipments.values()],
          ev_shipped: [...this.evShipped],
        };

  preparePlace = async (
    parentCustomer: d.CustomerId,
    orderId: d.OrderId,
    orderMethod: d.Method,
    orderNote: string,
  ): Promise<d.Result<d.PlaceResult, d.PlaceError>> => {
    const customer = this.customers.get(parentCustomer);
    return customer === undefined
      ? notFound("Place", "Customer", parentCustomer)
      : d.place(customer, orderId, orderMethod, orderNote);
  };
  commitPlace = async (r: d.PlaceResult): Promise<d.Result<void, d.PlaceError>> => {
    if (!this.check([[r.order.id, r.order_expected]], [])) return notEnabled("Place");
    this.orders.set(r.order.id, d.Order.toRow(r.order));
    return done;
  };

  preparePay = async (
    id: d.OrderId,
    paidAt: d.DateTime,
  ): Promise<d.Result<d.PayOutcome, d.PayError>> => {
    const state = this.order(id);
    return state === undefined ? notFound("Pay", "Order", id) : d.runPay(state, paidAt);
  };
  commitPay = async (r: d.PayOutcome): Promise<d.Result<void, d.PayError>> => {
    if (!this.check([[r.order.id, r.order_expected]], [])) return notEnabled("Pay");
    const before = this.orders.get(r.order.id);
    if (this.fault === "skip-core" && before !== undefined) {
      this.orders.set(r.order.id, { ...before, status: "paid" });
      return done;
    }
    this.orders.set(r.order.id, d.Order.toRow(r.order));
    return done;
  };

  prepareShip = async (
    id: d.OrderId,
    shipmentId: d.ShipmentId,
  ): Promise<d.Result<d.ShipOutcome, d.ShipError>> => {
    const state = this.order(id);
    return state === undefined ? notFound("Ship", "Order", id) : d.runShip(state, shipmentId);
  };
  commitShip = async (r: d.ShipOutcome): Promise<d.Result<void, d.ShipError>> => {
    if (!this.check([[r.order.id, r.order_expected]], [[r.shipment.id, r.shipment_expected]])) {
      return notEnabled("Ship");
    }
    this.orders.set(r.order.id, d.Order.toRow(r.order));
    if (this.fault !== "drop-row") {
      this.shipments.set(r.shipment.id, d.Shipment.toRow(r.shipment));
    }
    this.evShipped.push(r.ev_shipped);
    return done;
  };

  prepareSend = async (
    id: d.ShipmentId,
    ev: d.EvShipped,
  ): Promise<d.Result<d.SendOutcome, d.SendError>> => {
    if (!this.evShipped.some((e) => e.shipment_id === ev.shipment_id)) return notEnabled("Send");
    const row = this.fault === "ignore-id" ? [...this.shipments.values()][0] : this.shipments.get(id);
    return row === undefined
      ? notFound("Send", "Shipment", id)
      : d.runSend(stored(d.Shipment.fromRow(row)), ev);
  };
  commitSend = async (r: d.SendOutcome): Promise<d.Result<void, d.SendError>> => {
    if (!this.check([], [[r.shipment.id, r.shipment_expected]])) return notEnabled("Send");
    this.shipments.set(r.shipment.id, d.Shipment.toRow(r.shipment));
    return done;
  };

  prepareCancel = async (id: d.OrderId): Promise<d.Result<d.CancelOutcome, d.CancelError>> => {
    const state = this.order(id);
    if (state === undefined) return notFound("Cancel", "Order", id);
    const ran = d.runCancel(state);
    if (!ran.ok && ran.error.kind === "NotEnabled" && this.fault === "accept-disabled") {
      const row = { ...d.Order.toRow(state), status: "paid" as const };
      return d.runCancel(stored(d.Order.fromRow(row)));
    }
    return ran;
  };
  commitCancel = async (r: d.CancelOutcome): Promise<d.Result<void, d.CancelError>> => {
    if (this.fault !== "accept-disabled" && !this.check([[r.order.id, r.order_expected]], [])) {
      return notEnabled("Cancel");
    }
    this.orders.set(r.order.id, d.Order.toRow(r.order));
    return done;
  };
}
