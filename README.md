# rdra-ish

`rdra-ish` keeps requirements and design in one typed model, from the business value a
system serves down to its APIs, data, and state transitions, so you can check that each
layer is consistent with the layer that motivates it.

You write the model as code in a small DSL. The CLI type-checks it, finds gaps between
layers, and projects the same model into diagrams, review CSVs, state tables, and
exports such as OpenAPI, DBML, and TLA+.

## What is RDRA?

**RDRA** (Relationship-Driven Requirements Analysis) is a requirements modeling approach
that treats a system as a graph of typed elements (actors, businesses, BUCs, use cases,
screens, entities, and so on) linked by explicit predicates such as `performs`, `contains`,
and `creates`. The idea is to move from business-facing intent toward design review in one
model: each layer on the left explains why the layer on the right exists.

This tool implements **RDRA-ISH** (RDRA-inspired Implementation and System Heuristics).
It follows RDRA’s relationship-first style but is not a strict copy of the original method.
It adds vocabulary for system boundaries, APIs, access constraints, and entity lifecycle
so the same model can support `check`, diagrams, CSV reviews, and state derivation.

For how RDRA-ISH reads BUC, business flow, and use case, see
[RDRA-ish Interpretation](./docs/rdra-ish-interpretation.md).

The model follows a review spine: requirements, business, interaction, system
boundary, data, then lifecycle and rules. The
[Language Reference](./docs/language-reference.md#the-review-spine) is organized the
same way, and every kind in the language names the check or output that reviews it.

| Command | Reviews |
|---|---|
| `check`, `lint` | Type errors, unresolved references, and gaps between layers |
| `diagram` | RDRA layers, ER, state, and event views (Mermaid) |
| `csv`, `list` | CRUD matrix, permission audit, and element metadata |
| `states` | Reachable entity states and state rule violations |
| `export`, `verify` | OpenAPI, DBML, JSON Schema, AsyncAPI, and TLA+ with TLC |

## Recommended Modeling Loop

Refine the model in small stages instead of filling every layer at once. After each
stage, run `rdra-ish check` on the current `src/` tree and use diagrams or CSV output
to review only the concern you just added (scope, data touchpoints, boundaries,
structure, lifecycle, rules).

See [Incremental Modeling Flow](./docs/incremental-modeling.md) for stage goals,
placement rules, and validation commands. For a worked example across seven steps, use
[samples/incremental-order](./samples/incremental-order/).

## Installation

```sh
uv tool install rdra-ish
rdra-ish --help
```

## Quick Start

```sh
# 1) Validate model consistency
rdra-ish check samples/ec-site

# 2) Generate diagrams for review
rdra-ish diagram samples/ec-site --kind rdra --format mermaid --buc BucOrder
rdra-ish diagram samples/clinic-ops --kind event --format mermaid --buc BucAppointmentScheduling

# 3) Export coverage/access views
rdra-ish csv samples/clinic-ops --kind matrix
rdra-ish csv samples/clinic-ops --kind actor-permission-audit

# 4) Derive reachable states
rdra-ish states samples/clinic-ops --entity Appointment

# 5) Formal verification (requires `tlc` on PATH)
rdra-ish export samples/formal-verification/order.rdra --kind tla -o /tmp/rdra-tla
rdra-ish verify samples/formal-verification/order.rdra --backend tlc -o /tmp/rdra-tla
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
- [Formal Verification (TLA+/TLC)](./docs/formal-verification.md)
- [Diagram Sample Review Guide](./docs/diagram-sample-review.md)
- [Changelog](./CHANGELOG.md)
- Agent skills: [`skills/README.md`](./skills/README.md)

## Samples

- `samples/incremental-order`: seven-step refinement walkthrough
- `samples/ec-site`: compact end-to-end sample
- `samples/clinic-ops`: larger model with APIs, events, and access constraints
- `samples/personal-info`: personal data management sample
- `skills/rdra-ish-verify/samples`: TLA+/TLC examples (also linked from
  `samples/formal-verification/`; check files individually)

## Development

The dev environment is a Nix flake. It provides the Rust toolchain, maturin, uv,
Java, TLC, Node.js, and pre-commit.

```sh
nix develop          # or `direnv allow` with nix-direnv
cargo test --all
nix build            # builds the rdra-ish CLI into ./result
```

CI runs every step inside the same `nix develop` shell.

## Publish (maintainers)

```sh
uv tool install maturin
uvx maturin build --sdist
uvx maturin publish
```

## Project Layout

```text
crates/
  rdra-ish-syntax/   Lexer, parser, AST
  rdra-ish-core/     Semantic model, type checks, state derivation
  rdra-ish-emit/     Mermaid/CSV/state emitters
  rdra-ish-cli/      rdra-ish command
samples/
  clinic-ops/
  ec-site/
  personal-info/
```

## License

MIT
