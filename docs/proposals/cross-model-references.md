# Proposal: referencing another system's model

Status: draft for review. Nothing here is implemented.

<!-- constrained-by ../language-reference.md#modules-and-imports -->
<!-- constrained-by ../rdra-ish-interpretation.md -->
<!-- constrained-by ../vision.md -->

## Why

A product model must depend on another team's system model, for example the product's
checkout uses the authentication system's Login use case. Today the only way to do it is to
`import` the other model, which merges it into the product's model. Auth's use cases then run
as product steps, auth's lint findings and unreachable states show up in the product's
output, auth's `/login` lands in the product's OpenAPI, and the product can write auth's state
with no error. This proposal lets a product reference what another model publishes, checks
every reference, and keeps the other model out of the product's own results.

## Usage

The auth team publishes part of its model. One new predicate, `exposes`, names what other
models may reference.

```toml
# auth-model/rdra.toml
[model]
name = "auth"
version = "2.3.0"
```

```rdra
system AuthSystem "Authentication System"
exposes(AuthSystem, Login)
exposes(AuthSystem, Logout)
exposes(AuthSystem, Session)          // the whole state model of Session, edges of hidden use cases included
exposes(AuthSystem, EvSessionRevoked)
```

`rdra-ish list auth-model --kind surface` prints the published surface: each exposed use case
with the permissions it requires, the events it raises, and the state edges it makes, and
each exposed entity with every edge of its state model. Edges made by use cases that auth
does not expose read `(hidden)`.

The product team names the dependency once and binds it to an RDRA external system.

```toml
# shop-model/rdra.toml
[model]
name = "shop"
[dependencies]
auth = { path = "../auth-model", version = "2" }
```

```rdra
extsystem Auth "Authentication" from auth.AuthSystem

has_permission(Shopper, Auth.CanLogin)

usecase SignInAtCheckout "Sign in at checkout"
invokes(SignInAtCheckout, Auth.Login)

usecase PlaceOrder "Place order"
condition SignedIn "Shopper has an active session"
holds(SignedIn, Auth.Session.status == active)
applies_to(SignedIn, PlaceOrder)

usecase DropDraftOnRevoke "Drop the draft when the session is revoked"
triggers(Auth.EvSessionRevoked, DropDraftOnRevoke)
```

What the product author sees:

- `check` checks every `Auth.X` against the surface and reports nothing about auth's
  internals.
- `states` and `verify` treat auth's published edges as steps of the environment. A trace can
  show `Auth (hidden step) Session.status: active -> revoked [environment]`.
- OpenAPI, DBML, JSON Schema, CSV, and lint contain no auth element.
- The context and event diagrams draw `Auth` where an external system goes.

Mistakes get a diagnostic that names the fix:

| Written | Diagnostic |
|---|---|
| `Auth.Logon` | `UnknownForeignName`, with the closest exposed names |
| `Auth.Revoke` (exists, not exposed) | `NotExposed`, with a hint to ask auth's owners |
| `transitions(Auth.Session.status, PlaceOrder, active -> none)` | `ForeignWrite`, with the hint `invokes(PlaceOrder, Auth.Logout)` |
| `contains(BucCheckout, Auth.Login)` | `ForeignContained` |
| `import auth.core` across models | `ForeignImport`, with the hint to bind it with `extsystem ... from` |
| `holds(X, Auth.Session.status == expird)` | `UnreachableForeignValue` |

## Decisions

| Force | Decision | Why |
|---|---|---|
| Unit of foreignness | A model is a directory with `rdra.toml`. The tool finds it by walking up from the inputs, like Cargo. Without a manifest the tool behaves as today. | A module is one file and too small. Every input root is already a model in practice. |
| What a reference may read | Only the surface the owner exposes. An exposed entity publishes its whole state model, including edges of use cases that stay hidden. | Publishing only the edges of exposed use cases drops real moves (an unexposed `Revoke`), so the product's checks would miss alarms. |
| Binding | `extsystem Auth "..." from auth.AuthSystem`. Their `system` is our external system. | No new kind. The closure, the context and event diagrams, AsyncAPI, and the extsystem lint rules already treat an extsystem as the environment. |
| "Uses" | `invokes(UseCase, Auth.Login)`, widened from APIs to exposed foreign use cases. Inbound reactions use `triggers(Auth.Event, UseCase)`. | `invokes` already means "this use case calls a boundary". `tx.rs` groups only `invokes` whose target is an `api`, so the widened form adds no atomic write. `uses` stays a removed word. |
| Lifecycle | Auth's published edges become environment actions with no fairness. `invokes` is a product step whose effect is the invoked use case's published edge, in its own transaction. TLA+ splits `Next == ProductNext \/ EnvNext` with `WF_vars(ProductNext)` only. `--buc` keeps the environment. No finding is reported on a foreign subject. | Excluding auth freezes its state, and merging it counts its steps toward the product's liveness. Both give wrong verdicts today. |
| Stuck states | A product instance that can leave a state only after an environment step gets `StuckBehindEnvironment` (warning), with the environment steps marked in the trace. | A deadlock caused by the other system is a dependency risk, not a product bug. |
| Ownership | Any product write to a foreign entity is `ForeignWrite` (error). | Only the owner moves its state. The product asks for a move with `invokes`. |
| Transaction | A foreign call is never part of the product use case's transaction group. | A remote call is not atomic with local writes. |
| Permissions | The actors that run the calling use case must hold the permissions the invoked use case requires. The product grants them with `has_permission(Shopper, Auth.CanLogin)`, and the existing check reports a gap. Auth's internal permission gaps are never reported. | The product decides who may start the call. The existing access check covers it without a new rule. |
| Names and grammar | References are `Auth.Login`, `usecase::Auth.Login`, and `Auth.Session.status`. A foreign element's id is `Auth.Login`, which no local id can equal because local ids contain no `.`. Generated identifiers use `Auth__Login`. | `Auth.Login` parses today. The other two forms are the only new grammar. |
| Origin in the model | Foreign stubs live in each slot map behind a partition whose default iteration is local only. Opt-in views list foreign elements. A table of the predicates and argument positions that accept a foreign operand is the single source for the kind checker and the docs. A leak test runs every `diagram`, `export`, `csv`, and `list` kind and fails if an auth id appears where it must not. | About 100 element loops exclude foreign stubs without an edit. The 64 loops over relations can still meet a stub endpoint, which the table and the leak test bound. |
| Loading | One `load_project` in core, used by the CLI and the LSP, replaces the two include-path copies (`load.rs:40`, `backend.rs:688`). `files` lists dependency roots. Dependencies are paths in v1 and are not transitive. | The two copies already differ. A dependency in a sibling checkout or a submodule covers the first users. |
| Pinning | `rdra.lock` records a hash of each dependency's surface. When the surface changes, `check` fails with `SurfaceDrift` and a diff until the product re-locks. | Auth can change its internals freely. A change to what the product depends on is reviewed. |
| Outputs | Exclude by default. Opt in: access (gaps on calls), the context, event, usecase, and rdra diagrams (Auth as an external system, its elements as stubs), AsyncAPI (receive-only for foreign events), lifecycle and TLA+ (environment). `next` treats a foreign touchpoint as a satisfied boundary. | Each output either belongs to the product or shows a boundary. None shows auth's internals. |

## Deviation from RDRA

RDRA says an external system meets use cases through events. `invokes(UseCase, Auth.Login)`
lets a use case call a use case of an external system directly. The docs will record this as
an rdra-ish extension that applies only when the external system is bound to a model with
`from`. An extsystem without `from` keeps the event-only rule.

## Delivery

Each PR is useful on its own and verified through the CLI.

1. One loader: `core::project`, the manifest walk-up, CLI and LSP through it. Without a
   manifest every output is byte-identical to today.
2. Grammar: `usecase::Auth.Login`, `Auth.Session.status`, `extsystem ... from a.B`, the
   `exposes` signature. Parser tests only.
3. Publish: surface extraction with hidden edges, its hash, `list --kind surface`. The auth
   team can adopt this alone.
4. Bind and reference: path dependencies, partitioned stubs, the reference diagnostics,
   `reads`, `triggers`, `has_permission`, and `invokes` on foreign elements, and the leak
   test. Lifecycle refuses foreign guards until PR 5.
5. Environment: environment actions, the foreign transaction group, the TLA+ split,
   `StuckBehindEnvironment`, `UnreachableForeignValue`.
6. Permissions on calls and an `origin` column in the access audit.
7. Lock: `rdra.lock`, `SurfaceDrift`, and the re-lock command.

Later: git dependencies, LSP navigation into the other model, keys from a product entity to a
foreign entity, column-level exposure, fairness assumptions on the environment, transitive
dependencies.

## Open questions

1. Should the product call a foreign use case with the widened `invokes`, or with a new word
   such as `calls`? Both deviate from RDRA's event-only rule.
2. Should `StuckBehindEnvironment` be a warning, an error, or info?
3. Is a committed surface lock with a CI failure on drift the right gate, or is a declared
   version enough?
4. Are separate git repositories needed in v1, or do sibling paths and submodules suffice?
5. Does a product ever call another system under its own service identity instead of the
   shopper's?
6. Are keys from a product entity to a foreign entity (`Order.session_id`) needed in v1?
