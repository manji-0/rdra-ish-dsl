# Proposal: referencing another system's model

Status: draft for review, revision 2. Nothing here is implemented.

<!-- constrained-by ../language-reference.md#modules-and-imports -->
<!-- constrained-by ../rdra-ish-interpretation.md -->
<!-- constrained-by ../vision.md -->

## Why

A product model must depend on another team's system model. For example, checkout uses the
authentication system's Login use case, and order processing reads the payment system's
payment ledger. Today the only way is to `import` the other model, which merges it into the
product's model. Auth's use cases then run as product steps, its findings and APIs land in
the product's output, and the product can write auth's state with no error.

This proposal lets a product reference only what another model publishes, checks every
reference on both sides, and keeps the other model out of the product's own results.

## Decisions taken with the user

- A product calls a foreign use case with `invokes`, widened from APIs. The deviation from
  RDRA's rule that an external system meets use cases through events is documented.
- A product instance that can leave a state only after the other system acts gets a warning,
  `StuckBehindEnvironment`.
- A committed lock records a hash of each dependency's published surface. A change fails
  `check` with a diff until the product re-locks.
- Dependencies on other git repositories work in v1.
- A call can run under the product's own service identity, not only the human actor's.
- A product cannot reference another system's internal entities. It reads published
  entities, which the owner writes and does not read: an append-only log such as domain
  events, or silver data such as an Iceberg table.

## Usage

### The owner publishes

```toml
# auth-model/rdra.toml
[model]
name = "auth"
```

```rdra
system AuthSystem "Authentication System"

usecase Login "Log in"
usecase Revoke "Revoke a session"            // internal

entity Session "Session" {                   // internal: no other model can name it
  id: Int @pk
  account_id: Int
  status: Enum(none, active, revoked) @default(none)
}
transitions(Session.status, Login, none -> active)
transitions(Session.status, Revoke, active -> revoked)

// Published information (silver): auth writes it in the same transaction as Session
// and never reads it.
entity SessionView "Session (published)" {
  session_id: Int @pk
  account_id: Int
  status: Enum(active, ended)
}
writes(SessionApi, SessionView)
sets(Login, SessionView, status == active)
sets(Revoke, SessionView, status == ended)

exposes(AuthSystem, Login)                   // callable on behalf of an actor
exposes(AuthSystem, EvSessionEnded)
exposes(AuthSystem, SessionView).upsert()
```

A payment system in another repository publishes an append-only ledger and a use case that
only a service may call:

```rdra
exposes(PaymentSystem, CapturePayment).caller(system)
exposes(PaymentSystem, EvPaymentCaptured)
exposes(PaymentSystem, PaymentEntry).append()
```

`rdra-ish list auth-model --kind surface` prints the published surface as canonical text.
This is the same text the consumer's lock records:

```text
usecase Login "Log in" caller(actor) requires(CanLogin) raises(EvLoggedIn)
entity SessionView "Session (published)" upsert { session_id: Int @pk  account_id: Int  status: Enum(active, ended) }
writer SessionView Login: status == active
writer SessionView (unexposed): status == ended
```

The owner's own `check` enforces the published-entity rules from one table:

| Diagnostic | When |
|---|---|
| `PublishedEntityMutated` | an `append` entity is updated, or any published entity gets `deletes` or `transitions` |
| `PublishedEntityReadByOwner` | the owner reads a published entity: `reads`, `shows`, a field mapping, a `holds` body, or a constraint |
| `PublishedEntityKeysInternal` | a published entity keys to an internal entity |
| `PublishedNotInSourceTx` | a published row is not written in the same transaction as the internal change it reports |
| `InternalEntityExposed` | `exposes(AuthSystem, Session)` without a mode; the hint is to publish a projection |
| `PublishedEntityNeverWritten` | no owner use case writes the published entity |

The owner's lifecycle checks skip published entities, and a use case exposed with
`.caller(system)` is not reported as having no actor.

### The product depends

```toml
# shop-model/rdra.toml
[model]
name = "shop"
[dependencies]
auth     = { path = "../auth-model" }
payments = { git = "https://github.com/acme/payments-model.git", rev = "v1.4.0", dir = "model" }
```

```rdra
extsystem Auth "Authentication" from auth.AuthSystem
extsystem Pay "Payments" from payments.PaymentSystem
system ShopBackend "Shop backend"                          // the product's own service identity

has_permission(Shopper, Auth.CanLogin)                     // the shopper's grant
has_permission(ShopBackend, Pay.CapturePayments)           // the service's grant

invokes(SignInAtCheckout, Auth.Login)                      // runs as the shopper
relate(Order, Auth.SessionView, N:1).optional()            // a key to a published row
condition SignedIn "The order's session is active"
holds(SignedIn, Auth.SessionView.status == active)         // a guard on published data
applies_to(SignedIn, PlaceOrder)

triggers(EvOrderPlaced, CapturePaymentForOrder)
invokes(CapturePaymentForOrder, Pay.CapturePayment).as_service(ShopBackend)
triggers(Pay.EvPaymentCaptured, MarkPaid)                  // learn the outcome from an event
transitions(Order.status, MarkPaid, placed -> paid)
```

What the product author sees:

- `check` lists the environment once, `Auth (auth, path ../auth-model), Pay (payments, git
  4c8e1a7 v1.4.0)`, and reports nothing about auth's or payments' internals.
- `StuckBehindEnvironment_Order_status_placed` (warning). A placed order leaves `placed` only
  through `MarkPaid`, which runs only when `Pay` raises `EvPaymentCaptured`.
- The TLA+ export has `Next == ProductNext \/ EnvNext` and `WF_vars(ProductNext)`.
- OpenAPI, DBML, JSON Schema, CSV, and lint contain no foreign element. The DBML column
  `sessionview_session_id` has the note `external: auth.SessionView.session_id`. AsyncAPI
  lists `Pay.EvPaymentCaptured` as receive-only.
- The access audit has a principal-kind column and an origin column.
- `list --kind dependency` prints one row per foreign reference: how it is used, by which
  local element, and where.

Mistakes get a diagnostic that names the fix:

| Written | Diagnostic |
|---|---|
| `Auth.Logon` | `UnknownForeignName`, with the closest published names |
| `Auth.Session` | `NotExposed`, with the hint to read `Auth.SessionView` or react to `Auth.EvSessionEnded` |
| `updates(PlaceOrder, Auth.SessionView)` | `ForeignWrite`, with the exposed use case that makes the move |
| `contains(BucCheckout, Auth.Login)` | `ForeignContained` |
| `import auth.core` across models | `ForeignImport`, with the hint to bind it with `extsystem ... from` |
| `invokes(X, Pay.CapturePayment)` without `.as_service` | `CallerKindRejected` |
| `invokes(X, Auth.Login).as_service(ShopBackend)` | `CallerKindRejected` |
| a call whose principal lacks the permission | `MissingPermission`, naming the call and its principal |
| `holds(X, Auth.SessionView.status == expired)` | `UnreachableForeignValue` |

### Git, offline, and CI

```sh
rdra-ish deps lock            # network: resolve rev to a commit, fetch, extract the surface, write rdra.lock
rdra-ish deps fetch --locked  # CI: fetch exactly the locked commits; fail if rdra.toml and rdra.lock disagree
rdra-ish deps verify          # network: re-extract the surface from the locked commit and compare
rdra-ish check .              # never uses the network
```

## Decisions

| Force | Decision | Why |
|---|---|---|
| Unit of foreignness | A model is a directory with `rdra.toml`, found by walking up from the inputs. Without a manifest the tool behaves as today. | A module is one file and too small. |
| What a reference may read | Exposed use cases and events, and published entities. Internal entities are never visible. | The user's rule. A product learns foreign state from events and published entities. |
| Published entities | `exposes(System, Entity).append()` allows `creates` and `sets`. `.upsert()` also allows `updates` and `writes`. `deletes` and `transitions` are errors. The owner never reads one. A published row is written in the transaction of the internal change it reports. | Today a write to an entity that has no key to the changed entity lands in a separate lifecycle action (`Login_1`, `Login_2`), so the published row could disagree with the internal state. The rule keeps them in one action. |
| Binding | `extsystem Auth "..." from auth.AuthSystem`. Their `system` is our external system. | No new kind. The closure, the diagrams, AsyncAPI, and the extsystem lint rules already treat an extsystem as the environment. |
| Calls | `invokes(UseCase, Auth.Login)`. Inbound reactions use `triggers(Auth.Event, UseCase)`. A call is a product step with no product write: it may have no effect in the product's own state. | `tx.rs` groups only `invokes` whose target is an `api`, so the widened form adds no atomic write. A remote call is not atomic with local writes. |
| Principal | Without a chain the principal is the calling use case's actors. `.as_service(ShopBackend)` makes a product `system` the principal, and `has_permission(System, Ext.P)` grants it. The owner states who may call with `.caller(actor | system | any)`, default `actor`. Both cases go through the existing missing-permission check. | `as` is a keyword and `.by` already means medium context on `belongs`, `forbidden`, and `invariant`, so `.as_service` is the name with no clash. |
| Lifecycle | Every owner writer of a published entity, hidden or exposed, is an environment action with no fairness. A product use case that only a foreign event starts runs at the environment's pace. TLA+ splits `Next == ProductNext \/ EnvNext` with `WF_vars(ProductNext)`. `--buc` keeps the environment. No finding is reported on a foreign subject. | The lifecycle today never reads `triggers`, so an event-started use case counts toward product liveness. Writer-only environment actions over-approximate the owner, so safety results stay sound. |
| Stuck states | `StuckBehindEnvironment` (warning) when an instance can leave a state over all steps but not over product steps alone. Stuck over all steps stays today's `Deadlock`. | The existing stuck check runs twice, once on fair edges only. No new exploration. |
| Ownership | Any product write to a foreign element is `ForeignWrite`. | Only the owner writes its data. The product asks with `invokes`. |
| Names and grammar | References are `Auth.Login`, `usecase::Auth.Login`, and `Auth.SessionView.status`. A foreign id contains `.`, so no local id can equal it. Generated identifiers use `Auth__Login`. | `Auth.Login` parses today. The other two forms are the only new grammar. |
| Origin in the model | Foreign stubs live in each slot map behind a partition whose default iteration is local only. One table lists every predicate position that accepts a foreign operand and the check that reads it. A test runs every `diagram`, `export`, `csv`, and `list` kind with sentinel ids and fails on a leak. | About 100 element loops exclude stubs without an edit. The 64 relation loops can still meet a stub endpoint, which the table and the test bound. |
| Loading | One `load_project` in core for the CLI and the LSP replaces the two include-path copies. Dependencies are a path or `git` + `rev` + `dir`. Transitive dependencies are checked but not bindable. | The two copies already differ. |
| Lock | `rdra.lock` holds each dependency's commit, its canonical surface text, and its sha256. `check` and the LSP read the surface from the lock, so they need neither the network nor a cache. `deps lock` extracts the surface from the pinned sources and fails if the owner model has errors. `SurfaceChanged` prints a diff that marks the lines the product uses. `LockOutOfDate` and `LockTampered` also fail `check`. | The diff needs no old checkout, and CI needs no fetch to run `check`. |
| Boundary checks | Replace the `len() != 1` tests in `system.rs` with a `SystemMembership { None, One, Many }` value. | Those tests misfire once an element belongs to no local system. |
| Outputs | Exclude by default. Opt in: access (gaps on calls), the context, event, usecase, and rdra diagrams (the external system and its referenced elements as stubs), AsyncAPI (receive-only foreign events), lifecycle and TLA+ (environment). `next` treats a foreign touchpoint as a satisfied boundary. | Each output either belongs to the product or shows a boundary. None shows the owner's internals. |

## Deviation from RDRA

RDRA says an external system meets use cases through events. `invokes(UseCase, Auth.Login)`
lets a use case call a use case of an external system directly. The interpretation document
will record this as an rdra-ish extension that applies only when the external system is bound
to a model with `from`. An extsystem without `from` keeps the event-only rule.

## Delivery

Each PR is useful on its own and verified through the CLI.

1. One loader: `core::project`, the manifest walk-up, CLI and LSP through it. Every sample
   gives byte-identical output.
2. Publish (the owner can adopt this alone): `exposes` with `.append()`, `.upsert()`, and
   `.caller()`; the published-entity rules; the same-transaction rule (today's `Login_1` and
   `Login_2` become one TLA+ action); `list --kind surface`.
3. Path dependency, binding, and references: `extsystem ... from`, the partitioned stubs, the
   two new grammar forms, the reference diagnostics, `ForeignWrite`, widened `invokes`, the
   interpretation note, the leak test. The lifecycle refuses foreign guards until PR 6.
4. Lock: `rdra.lock`, `check` reading the lock, `SurfaceChanged`, `LockOutOfDate`,
   `LockTampered`, `deps lock`.
5. Principals: `.as_service`, `has_permission(System, Ext.P)`, caller kinds, the audit
   columns.
6. Environment: environment actions, environment pacing, the TLA+ split,
   `StuckBehindEnvironment`, `UnreachableForeignValue`, keys to published entities.
7. Git: `git` dependencies, the cache, `deps fetch [--locked]`, `deps verify`, LSP reload on a
   lock change. Tested with a local `file://` repository and a run with the network and cache
   removed.

Later: LSP navigation into the owner's files, fairness promises from an owner, column subsets
of a published entity.

## Open questions

1. Should there be a way to reference an internal entity directly? Options: none (publish a
   projection instead), a consumer-side `unpublished(Auth.Session)` with a permanent warning,
   or an owner-side `shares(AuthSystem, Session)` that the owner must consent to.
2. Should the owner state who may call per use case (`.caller(...)`) or per permission
   (`holder actor | service`)?
3. Should a published upsert entity also publish its state edges, so a product can rely on
   "an ended session never becomes active again", or stay writer-only (sound, with more false
   alarms)?
4. Should environment pacing of event-started use cases also apply to external systems
   without `from`? That changes some existing sample results.
5. May `rev` name a branch, or only a tag or a commit?
6. Should any surface change fail `check`, or only a change to the lines the product uses?
