# rdra-ish

`rdra-ish` keeps requirements and design in one typed model, from the business value a
system serves down to its APIs, data, and state transitions, so you can check that each
layer is consistent with the layer that motivates it.

You write the model as code in a small DSL. The CLI type-checks it, finds gaps between
layers, and projects the same model into diagrams, review CSVs, state tables, and
exports such as OpenAPI, DBML, SQL DDL, and TLA+.

## Where the Method Comes From

rdra-ish joins three sources in one model. Each source covers a different part of the
model, and that part keeps the names its source uses.

| Part of the model | Source | Elements |
|---|---|---|
| Requirements and business, above the system | RDRA | `requirement`, `goal`, `business`, `buc`, `flow`, `step`, `variation`, `condition`, `event`, `actor`, `extsystem` |
| From use case to design, below the system boundary | ICONIX robustness analysis | `usecase`, `screen`, `api`, `entity`, `field` |
| Lifecycle and verification | rdra-ish's own | lifecycles, state rules, temporal properties |

### Requirements and Business: RDRA

**RDRA** (Relationship-Driven Requirements Analysis, by Zenji Kanzaki) is a requirements
modeling approach. It treats a system as a graph of typed elements (actors, businesses,
BUCs, use cases, screens, entities, and so on) linked by explicit predicates such as
`performs`, `contains`, and `creates`. The idea is to move from business-facing intent
toward design review in one model: each layer on the left explains why the layer on the
right exists.

From RDRA, rdra-ish takes the layers above the system: the requirements and goals, the
businesses and their BUCs, each BUC's business flow with its steps, variations, and
conditions, and the actors, external systems, and events around the system. ICONIX has
none of these; it starts from a domain model and use cases.

rdra-ish also takes RDRA's central idea: one graph of typed elements, where each layer
gives the reason for the next. `check` and `next` read this graph to find the gaps between
layers. This idea shapes the whole model, including the parts below.

### From Use Case to Design: ICONIX

Below the system boundary, rdra-ish follows ICONIX, Doug Rosenberg's use-case-driven
process, more closely than RDRA. RDRA's UC複合図 connects a use case to its screens,
events, conditions, and information. ICONIX goes one step further with robustness
analysis, which comes from Ivar Jacobson's OOSE: it reads each use case as boundary,
control, and entity objects, then puts their messages in order in a sequence diagram, and
refines the domain model as it goes. rdra-ish keeps the three roles as elements of the
language:

| ICONIX role | rdra-ish element |
|---|---|
| Boundary | `screen`, `extsystem`, `timer` |
| Control | `api`, or the use case itself when it has no API |
| Entity | `entity`, with its `field`s |

So the outputs for engineers are the ones ICONIX produces:

- the robustness diagram (`diagram --kind robustness`);
- the sequence diagram of a use case or a BUC (`diagram --kind sequence`);
- the data model (`diagram --kind er`, and the DBML and SQL DDL exports).

Most of a model is written in this part. In the samples, about 1,000 declarations are use
cases, APIs, entities, and screens, against about 460 for BUCs, business flow steps,
businesses, requirements, and goals.

rdra-ish stops where ICONIX moves on to object-oriented design. It does not assign
operations to classes or draw class diagrams. It ends at the APIs (OpenAPI, AsyncAPI), the
data, and the states.

### Lifecycle and Verification: rdra-ish's Own

RDRA draws state models, but neither RDRA nor ICONIX checks behavior over time. rdra-ish
adds entity lifecycles, state rules, and temporal properties, and checks them:

- `states` derives the reachable states of each entity and reports the state rules they
  break.
- `verify` explores the states of the whole model and checks the properties; `--with-tlc`
  runs TLC beside it on the TLA+ export.
- `verify --prove` (experimental) proves state rules for any number of rows with z3.

[Formal Verification](./docs/formal-verification.md) describes this part.

### The Name

This tool implements **RDRA-ISH** (RDRA-inspired Implementation and System Heuristics).
The name comes from the first part, because RDRA's relationship-first graph shapes the
whole model. The method is not a strict copy of RDRA: the part below the system boundary
comes from ICONIX, and the lifecycle part and the access constraints (`permission`) are
rdra-ish's own.

For how RDRA-ISH reads BUC, business flow, and use case, see
[RDRA-ish Interpretation](./docs/rdra-ish-interpretation.md).

The model follows a review spine: requirements, business, interaction, system
boundary, data, then lifecycle and rules. The
[Language Reference](./docs/language-reference.md#the-review-spine) is organized the
same way, and every kind in the language names the check or output that reviews it.

| Command | Reviews |
|---|---|
| `check`, `lint` | Type errors, unresolved references, and gaps between layers |
| `next` | The refinement stage of each BUC, the gaps that block the next stage, and its questions |
| `diagram` | The RDRA 3.0 diagrams as Mermaid text for a reader that redraws them: system context, requirements, business, robustness, sequence, UC複合図, business flow, screen transitions, information, conditions, state, events, ER, and the whole RDRA graph. Each can be scoped to a BUC, a use case, an actor, or an entity, so it embeds in a PRD or a design document; `--all` writes the set in one run |
| `csv`, `list` | CRUD matrix, permission audit, and element metadata |
| `states` | Reachable entity states and state rule violations |
| `export`, `verify` | OpenAPI, DBML, SQL DDL, JSON Schema, AsyncAPI, and TLA+; `verify` checks every rule and temporal property with a native checker and no JVM, `--with-tlc` adds TLC as a second opinion, and `--prove` (experimental) proves state rules for any number of rows with z3 |
| `codegen` | The domain types a build imports: `--lang ts` prints TypeScript state, row, and id types that the [compatibility contract](./docs/compatibility.md#the-typescript-types-of-codegen) covers; the functions and the Rust and Go output are experimental |

## Documents from the Model

The diagrams and tables are made to go into the documents a team reads: a PRD for the
product side (system context, requirements, business flow, each BUC's steps, acceptance
rules) and a design document for the engineers (each BUC's robustness, screen, and
sequence diagrams, state models, the data model, rules, CRUD, and permissions). Each
embedded diagram is generated, scoped, and small enough to render, and its `%%` header
says how to regenerate it.

```sh
# Every per-BUC and per-entity diagram of the model, one .mmd each, ready to embed
rdra-ish diagram samples/clinic-ops --all --out docs/diagrams
# Tables for the same documents
rdra-ish list samples/clinic-ops --kind rule --format markdown
```

[PRD and Design Document Guide](./skills/rdra-ish-diagram/references/documents.md) gives
the base structure, section by section: which diagram or table goes where, and the rules
for embedding them. It is a base: a project's own document template or writing
instructions come first. [`samples/clinic-ops/prd.md`](./samples/clinic-ops/prd.md) and
[`design-doc.md`](./samples/clinic-ops/design-doc.md) are generated in that structure.

## Recommended Modeling Loop

Refine the model in small stages instead of filling every layer at once. After each
stage, run `rdra-ish check` on the current `src/` tree and use diagrams or CSV output
to review only the concern you just added (scope, screens, data touchpoints, API and
access boundaries, structure, lifecycle, rules). `rdra-ish next src/` tells you which stage each BUC has
reached, which elements block the next stage, and which questions to ask next.

See [Incremental Modeling Flow](./docs/incremental-modeling.md) for stage goals,
placement rules, and validation commands. For a worked example across seven steps, use
[samples/incremental-order](./samples/incremental-order/).

## Installation

```sh
uv tool install rdra-ish
rdra-ish --help
```

With Nix (flakes enabled), install `rdra-ish` and `rdra-ish-lsp` from this repository:

```sh
nix profile install github:manji-0/rdra-ish-dsl          # latest main, the early channel
nix profile install github:manji-0/rdra-ish-dsl/<tag>    # a release after v2.3.0
nix run github:manji-0/rdra-ish-dsl -- --help            # run without installing
```

The Nix package brings the `tlc` that `verify --with-tlc` runs and the `z3` that
`verify --prove` runs. A `tlc` or `z3` already on PATH takes precedence over them.

`main` is the early channel: it gets each change first, and may change between releases
what [Compatibility](./docs/compatibility.md) keeps across them. The `Unreleased` section
of the [changelog](./CHANGELOG.md) lists those changes.

rdra-ish follows semantic versioning from 1.0.0. [Compatibility](./docs/compatibility.md)
lists what a minor or patch release keeps: a model that loads keeps loading, and `check`
and `verify` give it the same verdict within the same printed bounds.

Upgrading a model written for 0.4.x: run `python3 scripts/migrate-syntax-v3.py <model dir>`,
then `rdra-ish check`. The [v1.0.0 notes](./CHANGELOG.md#v100---2026-11-03) list the
rewritten forms and the verdicts that change.

## Quick Start

```sh
# 1) Validate model consistency, and see the next refinement stage of each BUC
rdra-ish check samples/ec-site
rdra-ish next samples/ec-site

# 2) Generate diagrams for review
rdra-ish diagram samples/ec-site --kind context --format mermaid
rdra-ish diagram samples/ec-site --kind usecase --format mermaid --buc BucOrder
rdra-ish diagram samples/clinic-ops --kind flow --format mermaid --buc BucVisitCheckIn
rdra-ish diagram samples/clinic-ops --kind sequence --buc BucVisitCheckIn
rdra-ish diagram samples/clinic-ops --kind usecase --actor ReceptionClerk

# 3) Export coverage/access views (-o out writes out.matrix.csv, out.actor-permission-audit.csv)
rdra-ish csv samples/clinic-ops --kind matrix
rdra-ish csv samples/clinic-ops --kind actor-permission-audit

# 4) Derive reachable states
rdra-ish states samples/clinic-ops --entity Appointment

# 5) Formal verification (no JVM; --with-tlc also runs `tlc` or `tlc2` from PATH)
rdra-ish verify samples/formal-verification/order.rdra
rdra-ish verify samples/formal-verification/order.rdra --with-tlc -o /tmp/rdra-tla

# 6) TypeScript types of the entities and their states
rdra-ish codegen samples/ec-site --lang ts -o /tmp/domain.ts
```

## What You Can Review

- **Structure consistency**: type errors, unresolved references, duplicate definitions.
- **Coverage**: actor/use-case/API/entity links and CRUD matrix gaps.
- **Access design**: required permissions/media and actor-permission mismatches.
- **State design**: unreachable variants, missing creation paths, state rule violations.
- **Boundary design**: API/system/event inconsistencies and orphaned nodes.

## Learning Path

1. [Incremental Modeling Flow](./docs/incremental-modeling.md) + [samples/incremental-order](./samples/incremental-order/)
2. Look up syntax in [Language Reference](./docs/language-reference.md) only as needed
3. Install agent skills from [`skills/README.md`](./skills/README.md) (`rdra-ish-write`, `rdra-ish-review`)

## Main Documents

- [Incremental Modeling Flow](./docs/incremental-modeling.md) — start here
- [Language Reference](./docs/language-reference.md)
- [CLI Reference](./docs/cli-reference.md)
- [RDRA-ish Interpretation](./docs/rdra-ish-interpretation.md)
- [State Derivation](./docs/state-derivation.md)
- [Formal Verification](./docs/formal-verification.md)
- [Compatibility](./docs/compatibility.md) — what each release from 1.0.0 keeps stable
- [Diagram Sample Review Guide](./docs/diagram-sample-review.md)
- [PRD and Design Document Guide](./skills/rdra-ish-diagram/references/documents.md) — which
  diagrams and tables go into each section
- [Changelog](./CHANGELOG.md)
- Agent skills: [`skills/README.md`](./skills/README.md)

## Samples

- `samples/incremental-order`: seven-step refinement walkthrough
- `samples/ec-site`: compact end-to-end sample
- `samples/clinic-ops`: larger model with APIs, events, and access constraints, with a
  generated design document ([`design-doc.md`](./samples/clinic-ops/design-doc.md)) and PRD
  ([`prd.md`](./samples/clinic-ops/prd.md))
- `samples/patent-ops`, `samples/food-permit-ops`, `samples/insurance-claim-ops`: three
  Japanese business domains (a company's patent work, a 保健所's food business permits, a
  non-life insurer's motor claims), modeled from primary sources. `verify --deny-undecided`
  decides every check on each; insurance-claim-ops keeps one known deadlock, which its
  `// expect` header names
- `samples/oidc-auth`: an OpenID Provider with every OpenID Connect authentication scenario
  (code, implicit, and hybrid flows, PKCE, `prompt` errors, step-up, refresh rotation,
  revocation, device flow, CIBA, and RP-initiated, front-channel, and back-channel logout),
  with a [scenario table](./samples/oidc-auth/README.md#scenarios)
- `samples/personal-info`: personal data management sample
- `samples/api-contract`: API contract metadata and the OpenAPI export
- `skills/rdra-ish-verify/samples`: `verify` examples (also linked from
  `samples/formal-verification/` and, for a model that must fail,
  `samples/formal-verification-fail/`; check files individually)

## Development

The dev environment is a Nix flake. It provides the Rust toolchain, maturin, uv,
Java, TLC, Node.js, and pre-commit.

```sh
nix develop          # or `direnv allow` with nix-direnv
cargo test --all
nix build            # builds rdra-ish and rdra-ish-lsp into ./result
```

CI runs every step inside the same `nix develop` shell.

`pre-commit install` sets up the hook. Each commit runs the whitespace checks,
`cargo fmt`, `cargo clippy`, and the tests of the crates it touches
(`scripts/diff-test.sh`, inside `nix develop` so the codegen tests use the flake's Go).
CI runs the whole workspace.

The files in `samples/formal-verification/` are symlinks into
`skills/rdra-ish-verify/samples/`. On Windows, clone with
`git clone -c core.symlinks=true` (Developer Mode or an elevated shell), or Git writes
each one as a text file that holds the link target.

## Publish (maintainers)

```sh
uv tool install maturin
uvx maturin build --sdist
uvx maturin publish
```

## Project Layout

```text
crates/
  rdra-ish-syntax/   Lexer, parser, AST, removed-form table
  rdra-ish-core/     Semantic model, type checks, state derivation
  rdra-ish-emit/     Mermaid/CSV/state/TLA+/SQL and other export emitters, codegen
  rdra-ish-cli/      rdra-ish command
  rdra-ish-lsp/      Language server
editors/vscode/      VS Code extension for the language server
docs/                Language, CLI, state derivation, verification, compatibility
skills/              Agent skills
samples/             Example models (see Samples)
scripts/             Migration scripts, sample artifact and verification checks
```

## License

MIT
