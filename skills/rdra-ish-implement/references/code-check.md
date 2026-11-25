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

## Findings

A build error maps back to a use case: by the probe line it is on, or by the method or
contract it names. One block per use case and kind:

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
| `CodeUnmapped`, `CodeBuildFailed` | Errors that belong to no use case; a failed build with no error rdra-ish reads | Fix the build by hand |
| `CodeConformance` | `run`: the store differs from the model after a call | Read the first difference: the traces line, and the rows only the model or only the store holds |

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

The ledger says an obligation **has an owner**, never that it holds.

| Finding | Fix |
|---|---|
| `CodeObligationUnowned_<name>` (error) | Add the marker or the entry |
| `CodeObligationOwnerMissing_<name>` (error) | Point the owner at a file that exists |
| `CodeObligationUnknown_<name>` (warning) | Remove the owner of an obligation the model no longer has |
