# `check --code`

<!-- derived-from ../../../docs/cli-reference.md#check---code -->

`check --code` asks whether the service has what the model requires. It parses no service
code: it generates every file of the `[code.<lang>]` tables again and compares, and at
`build` and `run` it reads the compiler. It runs after the model check, and checks nothing
on a model that fails it.

| Level | Checks | Runs |
|---|---|---|
| `fresh` (default) | Each generated file is what the model generates now | Nothing |
| `build` | `fresh`, then the probe compiles | Rust `cargo check --test <probe>`; TypeScript `tsc --noEmit -p tsconfig.rdra.json`; Go `go test -run ^$` |
| `run` | `build`, then the conformance test passes | Rust `cargo test --test <probe> -- rdra_conformance`; TypeScript `node <probe>`; Go `go test -run ^TestRdraConformance$` |

A passing run prints one `ok:` line per file on stderr and ends with
`OK: code: what the model requires, at --code=<level>` on stdout.

At `run`, a coverage statement per table comes before that line: the bound (rows created per
entity, the Int range), whether the BFS behind the traces completed or stopped at its cap,
the moves run and left out, the refused calls tried, and the assumptions H1 to H5. A pass is
**bounded conformance**, never equivalence: the safety rules the model verified over tracked
columns hold for runs through the port, within that bound, if the service answers from the
tracked columns alone (H1), every reachable call is tried (H2), seed and snapshot reach the
real store (H3), every write goes through the port (H4), and the rules read tracked columns
only (H5). Say so when you report a pass.

A BFS stopped at its cap tries no refused call, so a service that runs a call the model
refuses passes. `run` then prints `warning[CodeCoverageIncomplete]` and still exits 0; raise
`conformance_states` in the table and run `codegen --code` again, or pass `--deny-incomplete`
in CI to fail on it.

After a passing run, the self-check runs the run command again on four mutant traces, each
wrong at one row (`self-check: 4 of 4 mutant traces failed at their changed row`). A
`CodeMutantSurvived` error means one passed. When the absent-row mutant passes, make the
service look rows up by the id it is given. For any other kind, make the run command run
the probe's test on the file `RDRA_ISH_TRACES` names and fail when that test fails.

## Findings

The compiler's whole output prints first. A build error then maps back to a use case: by the
probe line it is on, by the method, contract, or generated function it names (Go `RunPay`),
or, for a wrong number of arguments, by the generated function its line calls. One block per
use case and kind:

```text
error[CodeMissingUseCase_Ship]: use case Ship has no implementation in svc::service::Service
  model: model/order.rdra:22:1  usecase Ship "Ship"
  code:  src/service.rs:70:26  E0277: the trait bound `Service: ShipUseCase` is not satisfied
  fix:   impl ShipUseCase for svc::service::Service (prepare_ship, commit_ship)
FAIL: code: 1 error(s) at --code=build
```

| Finding | Cause | Fix |
|---|---|---|
| A file `is not what codegen generates`, with `file edited`, `model changed`, or `generator changed` | The file was edited, the model changed, or rdra-ish changed | `rdra-ish codegen model --code`; move hand-written code out of the file |
| `CodeMissingUseCase_<UC>` | The service does not implement the contract | Implement `<UC>UseCase` |
| `CodeIncomplete_<UC>` | Some methods are missing | Add the methods the `fix:` line names |
| `CodeSignature_<UC>` | A method takes or returns other types | Match the contract's signature |
| `CodeError_<UC>` | Another error on the use case's probe line or methods | Read the quoted compiler error |
| `CodeCommonJs` | TypeScript: the package makes the probe and the domain code CommonJS | Set `"type": "module"` in `package.json` |
| `CodeServiceNotFound` | The probe cannot resolve the type `service` names: it was renamed or moved | Set `service` (and `module`, `new`) in the table, then `rdra-ish codegen model --probe` |
| `CodeUnmapped`, `CodeBuildFailed` | Errors that belong to no use case, each listed; a failed build with no error rdra-ish reads | Fix the build by hand |
| `CodeConformance` | `run`: the store differs from the model after a call, or the service panicked or threw | Read the first difference: the traces line, and the `model only:` / `store only:` rows; or where it panicked |
| `CodeMutantSurvived` | `run`: a mutant trace of the self-check passed | The `fix:` line: look rows up by id, or make the run command use `RDRA_ISH_TRACES` |
| `CodeCoverageIncomplete` | `run`: the BFS behind the traces stopped at `conformance_states`; a warning, an error with `--deny-incomplete` | Raise `conformance_states`, then `codegen --code` |

## The obligation ledger

The header lists the obligations: what `check` and `verify` assume and no type enforces
(`OneTransaction`, `Unique_Payment_Order`, `Ref_Order_customer_id`, ...). Once one owner
exists, `check --code` requires an owner for each. An owner is a line
`rdra: obligation <name> [file]` after a comment token (`//`, `#`, `--`) anywhere under the
table's `root`, or an entry under `[code.obligations]`:

```toml
[code.obligations]
Unique_Payment_Order = "../service/migrations/003_payment.sql"
```

A name is letters, digits, and `_`; a marker inside a string literal (after an odd number of
`"` or `` ` `` on its line) is not one, so a script that prints markers owns nothing.

The ledger says an obligation **has an owner**, never that it holds. A `OneTransaction`
owner says who keeps it; what `run` shows of it is the interleaved commits, which the
optimistic write passes (`UPDATE .. WHERE id = ? AND status = ?`, no row matched is
`NotEnabled`). The harness is one thread, so held locks and races are not shown.

## What a pass does not show

- `build` compiles the probe and what it imports, not the rest of the application.
- Each trace starts from a fresh store: `new` runs once per trace (181,095 times on
  clinic-ops at the default `conformance_states`), and `seed` empties the store. Share one
  database and keep `new` cheap. Migrations and data an earlier version stored are not run.
- A raised event is never consumed: the raise stays recorded on its row (a delete does not
  clear it), and the use case it triggers may run there again. Keep it as long as the row's
  history matters; `snapshot` lists it.

| Finding | Fix |
|---|---|
| `CodeObligationUnowned_<name>` (error) | Add the marker or the entry |
| `CodeObligationOwnerMissing_<name>` (error) | Point the owner at a file that exists |
| `CodeObligationUnknown_<name>` (warning) | Remove the owner of an obligation the model no longer has |
