# rdra-ish-dsl

RDRA-ISH stands for **RDRA-inspired Implementation and System Heuristics**.
It is not a strict implementation of the original RDRA scope; it is an
RDRA-inspired framework for carrying requirements work forward into system
boundaries, API boundaries, domain modeling, and an implementation-oriented
design overview.

This repository provides a DSL and compiler for describing those RDRA-ISH models.
You declare actors, entities, use cases, and so on as typed instances, and express
relationships between them with predicate calls.
It treats a model as source code: the compiler type-checks relationships,
generates reviewable artifacts, and reports model gaps such as unreachable states or
violated state constraints. It generates PlantUML / Mermaid diagrams (ER, RDRA,
layered object graph, state machine, sequence, event-flow) and CSV (actor list,
entity list, CRUD matrix), and
derives **the reachable state patterns of each entity from BUC patterns**.
An `api` element lets you express the API layer between screens and entities — the sequence
diagram renders the full `Actor → Screen → API → Entity` lane automatically.

<!-- derived-from ./docs/incremental-modeling.md#api-boundary-rules -->
<!-- derived-from ./docs/language-reference.md#relationship-predicates -->
## Layer Positioning

RDRA-ISH keeps the original RDRA-style idea that the layers on the left explain
the reason the layers on the right exist. The difference is that RDRA-ISH does not
stop at business-oriented requirements organization: it adds implementation design
vocabulary where it can still fit naturally inside the RDRA world.

![RDRA-ISH layer positioning](docs/assets/rdra-ish-layer-positioning.png)

RDRA-ISH therefore treats a model as a bridge: `check`, `diagram`, `csv`, and
`states` let a developer refine requirements, system boundaries, API boundaries,
entity structure, and lifecycle constraints in one source model.

For the intended reading of BUCs, business flow, and use cases, see
[RDRA-ish Interpretation](./docs/rdra-ish-interpretation.md). It explains where
RDRA-ish deliberately differs from a stricter reading of the original RDRA artifacts.

<!-- derived-from ./docs/language-reference.md -->
<!-- derived-from ./docs/state-derivation.md -->
## What It Helps You Check

- **Relationship consistency**: predicate arguments are type-checked, duplicate
  definitions are reported, imports are resolved, and ambiguous references can be
  disambiguated with `kind::Id` syntax.
- **Use-case coverage**: BUC-scoped diagrams and CRUD matrices show which actors,
  use cases, screens, APIs, and entities are actually connected.
- **Access coverage**: screen constraints show which UC/API permission and medium
  requirements pass through each screen, permission-callable lists show what each
  permission enables, actor-permission audits infer missing/excess actor grants, and
  `check` warns when those assignments do not match modeled operation paths.
- **Entity state reachability**: `states` computes which Enum / Bool / nullable /
  comparison-proposition combinations can be reached through declared use cases and
  events.
- **Model gaps**: diagnostics call out unreachable enum variants, missing creation
  paths, entity constraint violations, orphaned APIs, event-flow gaps, permission
  mismatches, cross-system coordination gaps, and FK-isolated writes in inferred
  transaction groups.
- **Review artifacts**: Mermaid is the lowest-friction default for text review, while
  PlantUML/SVG/PNG are available when a rendered asset is needed.

<!-- derived-from ./docs/language-reference.md#instance-declarations -->
<!-- derived-from ./docs/language-reference.md#access-constraints -->
<!-- derived-from ./docs/language-reference.md#belongs-context -->
<!-- derived-from ./docs/incremental-modeling.md#buc-context-and-access-rules -->
## Context and Access Modeling

RDRA-ISH keeps short labels for diagrams, but every instance can also carry a
longer `description`. Use it for review notes or domain explanation that should stay
in the source model without overcrowding generated diagrams:

```rdra
buc BucAppointmentScheduling "Appointment Scheduling" description "Booking, rescheduling, cancellation, and no-show handling."
api BookingApi "Booking API" description "Consistency boundary for appointment slot reservation."
```

Business-BUC mappings can describe the business context in which the BUC applies.
Use typed context values when the vocabulary should be reused, or string literals
when the context is still provisional:

```rdra
timing AppointmentRequested "Appointment Requested"
location FrontDesk "Front Desk"
medium StaffTerminal "Staff Terminal"

belongs(BucAppointmentScheduling, ClinicOps)
  .when(AppointmentRequested)
  .where(FrontDesk)
  .by(StaffTerminal)
```

- `.when(...)` records timing, trigger, or business situation.
- `.where(...)` records place, channel, organization point, or usage scene.
- `.by(...)` records the physical medium, device, terminal, or operating medium.

Permissions are modeled as vocabulary too. Actors receive permissions with
`has_permission`, and use cases or APIs declare what permission and medium they
require. Screens do not define those constraints directly; `csv --kind
screen-constraints` derives the screen paths from `displays(UC, Screen)` and
`invokes(UC, Api)`. `csv --kind permission-callables` and `list --kind
permission-callables` invert the same model so reviewers can see which use cases and
APIs each permission enables, including which UC->API paths carry API-level
requirements. `actor-permission-audit` projects those requirements back onto actors and
marks each actor/permission pair as `ok`, `missing`, or `excess`.

```rdra
actor Staff "Staff"
permission ScheduleWrite "Schedule Write"
medium StaffTerminal "Staff Terminal"

has_permission(Staff, ScheduleWrite)
requires_permission(BookAppointment, ScheduleWrite)
requires_medium(BookAppointment, StaffTerminal)
requires_permission(BookingApi, ScheduleWrite)
requires_medium(BookingApi, StaffTerminal)
```

## Installation

```sh
cargo install --path crates/rdra-ish-cli
```

<!-- derived-from ./docs/cli-reference.md -->
## Recommended Modeling Loop

<!-- derived-from ./docs/incremental-modeling.md -->
<!-- derived-from ./docs/incremental-modeling.md#stage-map -->

The modeling loop is intentionally staged. At each stage, ask only for the next
missing information, validate the current abstraction, then add the next level of
detail. Read the stages as a gradual shift from business concerns to technical
concerns: first name value, actors, use cases, BUC context, and access constraints;
then introduce data touchpoints, interaction/API boundaries, entity structure,
lifecycle, and enforceable rules.

![RDRA-ISH current spec and concretization steps](docs/assets/rdra-ish-spec-and-steps.png)

1. Declare shared actors, businesses, and entities under a shared module.
2. Add one BUC file at a time with its use cases, context, screens, CRUD predicates, and events.
3. Run `rdra-ish check <model-root>` after each BUC to catch type and import mistakes.
4. Generate Mermaid diagrams for quick review:
   `rdra-ish diagram <model-root> --kind rdra --format mermaid --buc <BucId>`.
5. Run `rdra-ish csv <model-root> --kind matrix` to review use-case/entity CRUD coverage.
6. Run `rdra-ish csv <model-root> --kind screen-constraints` to review inferred
   permission and medium paths through screens, use cases, and APIs.
7. Run `rdra-ish csv <model-root> --kind permission-callables` to review the
   permission-to-UC/API map, including API requirements projected onto invoking UC paths.
8. Run `rdra-ish csv <model-root> --kind actor-permission-audit` to review inferred
   missing and excess actor-side permission assignments.
9. Run `rdra-ish check <model-root>` to list whole-model consistency warnings across
   permissions, API/system boundaries, event-flow, transaction inference, and states.
10. Run `rdra-ish states <model-root>` to find unreachable states, missing creation
   paths, and state constraint violations.
11. Add `forbidden` / `invariant` / `required` / `exclusive` constraints when the model
   needs to assert invalid, conditional, mandatory, or mutually exclusive state facts.

For a slower abstract-to-concrete workflow, see
[Incremental Modeling Flow](./docs/incremental-modeling.md).
It also defines the recommended model directory layout and when to split shared files.

## Basic Usage

```sh
# Validate model
rdra-ish check src/

# Review structure and boundaries
rdra-ish diagram src/ --kind rdra --format mermaid
rdra-ish diagram src/ --kind sequence --format mermaid --buc BucOrder

# Review access and coverage
rdra-ish csv src/ --kind matrix
rdra-ish csv src/ --kind actor-permission-audit

# Derive reachable states
rdra-ish states src/ --entity Order
```

CLI options and all output kinds are documented in
[CLI Reference](./docs/cli-reference.md).

---

## Documentation

README keeps only a quick overview. Use these documents for specialized details:

- [CLI Reference](./docs/cli-reference.md): all subcommands/options, output kinds, and environment variables.
- [Language Reference](./docs/language-reference.md): DSL syntax, predicates, access/context modeling, and constraints.
- [Incremental Modeling Flow](./docs/incremental-modeling.md): staged modeling loop from business scope to technical rules.
- [State Pattern Derivation](./docs/state-derivation.md): `states` algorithm, diagnostics, and output semantics.
- [RDRA-ish Interpretation](./docs/rdra-ish-interpretation.md): how RDRA-ish reads BUC / business flow / UC concepts.

---

## Samples

- `samples/ec-site`: compact end-to-end sample for quick experimentation.
- `samples/clinic-ops`: larger connected model for BUC-scoped reviews and event-driven flows.

Useful sample entry points:

```sh
rdra-ish check samples/ec-site
rdra-ish diagram samples/ec-site --kind rdra --format mermaid --buc BucOrder

rdra-ish check samples/clinic-ops
rdra-ish diagram samples/clinic-ops --kind event-flow --format mermaid
rdra-ish states samples/clinic-ops --entity Appointment
```

The clinic sample design narrative is here:
[samples/clinic-ops/design-sample.md](./samples/clinic-ops/design-sample.md).

---

## Project layout

```
crates/
  rdra-ish-syntax/   Lexer · Parser · AST
  rdra-ish-core/     Semantic model · type checking · state pattern derivation
  rdra-ish-emit/     PlantUML / Mermaid / CSV / state-pattern emitters
  rdra-ish-render/   plantuml.jar wrapper
  rdra-ish-cli/      `rdra-ish` CLI
samples/
  clinic-ops/    Larger clinic operations sample (9 BUCs · APIs · event flows · access constraints)
  ec-site/       E-commerce site sample (BUCs · entities · state transitions)
  personal-info/ Personal data management sample
```

## License

MIT
