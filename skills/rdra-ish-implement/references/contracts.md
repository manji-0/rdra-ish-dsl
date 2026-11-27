# Contracts

<!-- derived-from ../../../docs/cli-reference.md#the-code-tables -->
<!-- derived-from ../../../docs/cli-reference.md#codegen -->

What to put in `[code.<lang>]`, what `codegen` prints, and how a service implements a use
case against it.

## The table

```toml
[code.ts]
domain = "../web/src/domain.ts"
traces = "../web/src/traces.txt"
probe = "../web/src/rdra_contract.ts"
service = "../web/src/order_service.ts#OrderService"

[code.go]
domain = "../shop/domain/domain.go"
traces = "../shop/svc/traces.txt"
probe = "../shop/svc/rdra_contract_test.go"
service = "example.com/shop/svc.Service"
new = "svc.New()"
```

| Key | Meaning |
|---|---|
| `domain` | The generated domain code. Required. |
| `traces` | The conformance traces; with it, `domain` also holds the conformance port and harness. |
| `probe` | The file stating that `service` implements each use case's contract: Rust, a file under the crate's `tests/`; TypeScript, a `.ts` file; Go, a `_test.go` in the service's package. |
| `service` | Rust: the type's path (`svc::service::Service`). TypeScript: its file and export (`path#Type`). Go: `<import path>.<Type>`. |
| `module` | Rust: the generated module's path (`svc::domain`). Go: `domain`'s import path. |
| `new` | An expression making a fresh service, for the conformance test. Required in Rust. |
| `package` | Go: the package name of `domain`. Default `domain`. |
| `seal`, `tsconfig` | TypeScript: write `domain` as `codegen --seal` does; the tsconfig the probe's `tsconfig.rdra.json` extends. |
| `root` | Where the build runs. Default: the nearest directory above `domain` with `Cargo.toml`, `go.mod`, `package.json`, or `tsconfig.json`. |
| `build`, `run` | A command list replacing the language's default. |
| `conformance_states` | The states the BFS behind the traces explores at most. Default `65536`. |

An unknown key fails every loading command with the line. The TypeScript table is
`[code.ts]`, whichever spelling `--lang` takes. TypeScript `run` needs Node 22.18 or later,
and the service's imports name the `.ts` files.

## What `codegen` prints

| | Rust | TypeScript | Go |
|---|---|---|---|
| State type | enum of structs, `kind()` | union of `Readonly` types told apart by `kind` | interface per state, `Kind()`, `<Entity>Visitor[R]` |
| Row and parse | `<Entity>Row`, `Order::from_row`, `into_row` | `<Entity>Row`, `Order.fromRow`, `Order.toRow`, `parseOrderRow(unknown)` | `<Entity>Row`, `OrderFromRow`, `OrderToRow` |
| Ids and scalars | `OrderId::from(7)`, `"12.50".parse::<Money>()` | `OrderId.parse(1)`, `Date.parse(..)`, each a `Result` | `ParseOrderId`, `ParseMoney(..)` |
| Dispatcher | `run_pay` | `runPay` | `RunPay` |
| Error | `Result<_, PayError>` | `Result<PayOutcome, PayError>` | `(result, error)`, `*PayError` with a `Kind` |
| Contract | trait `PayUseCase`: `prepare_pay`, `commit_pay` | type `PayUseCase`: `preparePay`, `commitPay`, each a `Promise` | interface `PayUseCase`: `PreparePay`, `CommitPay` |
| Conformance | `conform(\|\| Svc::new(), &traces)` | `await conform(() => newService(), traces)` | `Conform(func() ConformancePort {..}, traces)` |

- A state type exists only for an entity whose Enum column `transitions` drive. Only a
  transition function and `fromRow` make a state, so a call from a wrong state does not
  compile. A rule narrows a nullable column in a state (`invariant(..).when(status == paid)
  .then(paid_at == present)` makes `paid_at` present there).
- A use case gets one transition function per combination of its rows' source states. It
  takes the rows, then one input per value the model leaves open (a `present`, a chosen
  variation, a created row's free column).
- `<UseCase>Error` is `NotEnabled` (the use case does not run from these states),
  `Rejected` (a condition applied to it fails), or `InvalidArgument` (a row breaks what the
  model keeps). The generated code never panics or throws, except TypeScript's
  `assertNever`.
- A use case actors perform takes a caller first: `by: <UseCase>Caller`, made only from
  `<Actor>Capability::issue()` (Go `Issue<Actor>Capability()`). Whether the actor was
  authenticated is the service's job. In Go, refuse a call when `CallerValid(by)` is false.
- A use case split into transactions has one function per step, linked by run marks the
  service stores. An event a use case raises is a value the triggered use case takes.
- A use case whose vocabulary is not printed yet gets no function; the header says why, and
  the service writes it.

## A use case in the service

`prepare` reads and decides; `commit` writes.

1. `prepare`: load the rows by id, convert each with `fromRow`, call the dispatcher, and
   return its result. Write nothing. An absent row is `NotEnabled`.
2. `commit`: in one transaction, write each row only where it still is what the result's
   `<row>_expected` names: `UPDATE .. WHERE id = ? AND status = ?`. A write that matches no
   row rolls back and is `NotEnabled`. Hold what `prepare` read until then (the
   `OneTransaction` obligation). The expected source per language:

   | | Created row | Row in a state | Record |
   |---|---|---|---|
   | Rust `ExpectedSource` | `Absent` | `In { column, value }` | `Present` |
   | TypeScript `ExpectedSource` | `{ kind: "absent" }` | `{ kind: "in", column, value }` | `{ kind: "present" }` |
   | Go `ExpectedSource` | `{Absent: true}` | `{Column: "status", Value: "paid"}` | `{}` |

3. `ConformancePort`: `seed` empties the store and stores a snapshot's rows; `snapshot`
   returns every row (state types as `<Entity>Row`), every pending run mark, and every
   raised event a triggered use case reads. Nothing consumes a raised event: it stays on its
   row. The harness makes a port with `new` once per trace, so keep `new` cheap (one shared
   database that `seed` clears).

The harness runs each trace on a fresh service, compares the store with the model's state
after every call, and expects each call the model refuses to fail and change nothing. It
checks what is stored, not how.

```rust
impl PayUseCase for Service {
    fn prepare_pay(&mut self, order: OrderId, order_paid_at: DateTime) -> Result<PayOutcome, PayError> {
        run_pay(self.order(order, "Pay")?, order_paid_at)
    }

    fn commit_pay(&mut self, write: PayOutcome) -> Result<(), PayError> {
        let PayOutcome::Ran(r) = write;
        let id = *r.order.id();
        self.check("Pay", &[(id, r.order_expected)], &[])?;
        self.orders.insert(id.get(), Order::Paid(r.order).into_row());
        Ok(())
    }
}
```

`self.order` loads a row and converts it with `Order::from_row`; `self.check` is the
conditional write of step 2.
