// Service of tests/codegen_commit.rs: an in-memory store around the TypeScript that `codegen`
// prints for commit.rdra, run through the generated harness. Every commit is one call of the
// generated `commitWrites` on the result's `writes`: the store implements `CommitStore` once,
// over every entity, run mark and event, so no commit names a result's fields. An id is never
// used twice: a deleted row's id stays taken, as the model's slot does.
// FAULT=keep-mark plants one bug: the store never deletes a run mark.
import * as d from "./domain";

// What the service needs of Node, declared here: the repository installs no @types/node.
declare const require: (module: "node:fs") => { readFileSync: (path: string, encoding: "utf8") => string };
declare const process: { argv: string[]; env: Record<string, string | undefined>; exitCode: number | undefined };
const fs = require("node:fs");

type Tables = {
  accounts: Map<number, d.Account>;
  orders: Map<number, d.OrderRow>;
  shipments: Map<number, d.ShipmentRow>;
  marks: Map<string, number>;
  evShipped: d.EvShipped[];
  used: Set<string>;
};

const empty = (): Tables => ({ accounts: new Map(), orders: new Map(), shipments: new Map(), marks: new Map(), evShipped: [], used: new Set() });
const copy = (t: Tables): Tables => ({
  accounts: new Map(t.accounts),
  orders: new Map(t.orders),
  shipments: new Map(t.shipments),
  marks: new Map(t.marks),
  evShipped: [...t.evShipped],
  used: new Set(t.used),
});
const markKey = (usecase: string, entity: string, id: number): string => `${usecase} ${entity} ${id}`;
const ok = <T>(value: T): d.Result<T, never> => ({ ok: true, value });
const notEnabled = (usecase: string): d.Result<never, d.NotEnabled> => ({ ok: false, error: { kind: "NotEnabled", usecase } });

/** The tag of a row ("" for a record), or undefined when there is no such row. */
function tag(t: Tables, w: d.RowWrite): string | undefined {
  switch (w.entity) {
    case "Account":
      return t.accounts.has(w.id) ? "" : undefined;
    case "Order":
      return t.orders.get(w.id)?.status;
    case "Shipment":
      return t.shipments.get(w.id)?.status;
    default:
      return d.assertNever(w);
  }
}

function holds(t: Tables, w: d.RowWrite): boolean {
  const now = tag(t, w);
  switch (w.expected.kind) {
    case "absent":
      return !t.used.has(`${w.entity} ${w.id}`);
    case "present":
      return now !== undefined;
    case "in":
      return now === w.expected.value;
    default:
      return d.assertNever(w.expected);
  }
}

function store(t: Tables, keepMarks: boolean): d.CommitStore<never> {
  return {
    writeRow: async (w) => {
      if (!holds(t, w)) return ok(false);
      if (w.kind === "delete") {
        if (w.entity === "Account") t.accounts.delete(w.id);
        if (w.entity === "Order") t.orders.delete(w.id);
        if (w.entity === "Shipment") t.shipments.delete(w.id);
        return ok(true);
      }
      t.used.add(`${w.entity} ${w.id}`);
      if (w.entity === "Account") t.accounts.set(w.id, w.row);
      if (w.entity === "Order") t.orders.set(w.id, w.row);
      if (w.entity === "Shipment") t.shipments.set(w.id, w.row);
      return ok(true);
    },
    writeMark: async (w) => {
      const key = markKey(w.mark.usecase, w.mark.entity, w.mark.id);
      const now = t.marks.get(key);
      if (w.kind === "delete") {
        if (now !== w.mark.step) return ok(false);
        if (!keepMarks) t.marks.delete(key);
        return ok(true);
      }
      if (now !== (w.mark.step === 1 ? undefined : w.mark.step - 1)) return ok(false);
      t.marks.set(key, w.mark.step);
      return ok(true);
    },
    recordEvent: async (e) => {
      t.evShipped.push(e);
      return ok(undefined);
    },
  };
}

/** A value the store holds: rows it wrote went through the core, so they parse. */
function stored<T, E>(r: d.Result<T, E>): T {
  if (!r.ok) throw new Error(`the store holds an invalid row: ${JSON.stringify(r.error)}`);
  return r.value;
}

function service(keepMarks: boolean): d.ConformancePort {
  let t = empty();
  /** Commits any result's writes in one transaction: the tables as they were come back on an error. */
  const commit = async (writes: d.Writes): Promise<d.Result<void, d.NotEnabled>> => {
    const saved = copy(t);
    const r = await d.commitWrites(store(t, keepMarks), writes);
    if (!r.ok) t = saved;
    return r;
  };
  const order = (id: d.OrderId, usecase: string): d.Result<d.Order, d.NotEnabled> => {
    const row = t.orders.get(id);
    return row === undefined ? notEnabled(usecase) : ok(stored(d.Order.fromRow(row)));
  };
  return {
    seed: async (rows) => {
      t = empty();
      for (const a of rows.account) {
        t.accounts.set(a.id, a);
        t.used.add(`Account ${a.id}`);
      }
      for (const o of rows.order) {
        t.orders.set(o.id, o);
        t.used.add(`Order ${o.id}`);
      }
      for (const s of rows.shipment) {
        t.shipments.set(s.id, s);
        t.used.add(`Shipment ${s.id}`);
      }
      for (const m of rows.ship_order_runs) t.marks.set(markKey("Ship", "Order", m.id), m.step);
      t.evShipped = [...rows.ev_shipped];
    },
    snapshot: async () => ({
      account: [...t.accounts.values()],
      order: [...t.orders.values()],
      shipment: [...t.shipments.values()],
      ship_order_runs: [...t.marks.entries()].map(([key, step]) => ({
        id: stored(d.OrderId.parse(Number(key.split(" ")[2]))),
        step,
      })),
      ev_shipped: [...t.evShipped],
    }),
    preparePlace: async (parent, orderId) => {
      const account = t.accounts.get(parent);
      return account === undefined ? notEnabled("Place") : d.place(account, orderId);
    },
    commitPlace: (w) => commit(d.PlaceResult.writes(w)),
    preparePay: async (id) => {
      const o = order(id, "Pay");
      return o.ok ? d.runPay(o.value) : o;
    },
    commitPay: (w) => commit(d.PayOutcome.writes(w)),
    prepareShip1: async (id) => {
      if (t.marks.has(markKey("Ship", "Order", id))) return notEnabled("Ship");
      const o = order(id, "Ship");
      return o.ok ? d.runShip1(o.value) : o;
    },
    commitShip1: (w) => commit(d.Ship1Outcome.writes(w)),
    prepareShip2: async (mark, shipmentId) => {
      if (t.marks.get(markKey("Ship", "Order", mark)) !== 1) return notEnabled("Ship");
      return d.ship2({ usecase: "Ship", step: 1, id: mark }, shipmentId);
    },
    commitShip2: (w) => commit(d.Ship2Result.writes(w)),
    prepareSend: async (id, ev) => {
      const row = t.shipments.get(id);
      const raised = t.evShipped.some((e) => e.shipment_id === ev.shipment_id);
      if (row === undefined || !raised) return notEnabled("Send");
      return d.runSend(stored(d.Shipment.fromRow(row)), ev);
    },
    commitSend: (w) => commit(d.SendOutcome.writes(w)),
    prepareCancel: async (id) => {
      const o = order(id, "Cancel");
      return o.ok ? d.runCancel(o.value) : o;
    },
    commitCancel: (w) => commit(d.CancelOutcome.writes(w)),
    preparePurge: async (id) => {
      const o = order(id, "Purge");
      return o.ok ? d.runPurge(o.value, [...t.shipments.values()]) : o;
    },
    commitPurge: (w) => commit(d.PurgeOutcome.writes(w)),
  };
}

async function main(): Promise<void> {
  const path = process.argv[2];
  if (path === undefined) throw new Error("the traces file");
  const traces = fs.readFileSync(path, "utf8");
  const keepMarks = process.env["FAULT"] === "keep-mark";
  const r = await d.conform(() => service(keepMarks), traces);
  if (r.ok) {
    const { traces: t, calls, refusals, interleavings } = r.value;
    console.log(`ok ${t} traces, ${calls} calls, ${refusals} refusals, ${interleavings} interleavings`);
  } else {
    console.log(`FAIL ${r.error}`);
    process.exitCode = 1;
  }
}

void main();
