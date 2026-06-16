# rdra-ish

`rdra-ish` is a CLI tool and DSL compiler for reviewing requirements models as code.

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

It lets you model actors, use cases, APIs, screens, entities, and relationships, then:
- type-check the model (`check`)
- generate diagrams (`diagram`)
- export review CSVs (`csv`)
- derive reachable entity states (`states`)

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
rdra-ish diagram samples/clinic-ops --kind sequence --format mermaid --buc BucAppointmentScheduling

# 3) Export coverage/access views
rdra-ish csv samples/clinic-ops --kind matrix
rdra-ish csv samples/clinic-ops --kind actor-permission-audit

# 4) Derive reachable states
rdra-ish states samples/clinic-ops --entity Appointment
```

## What You Can Review

- **Structure consistency**: type errors, unresolved references, duplicate definitions.
- **Coverage**: actor/use-case/API/entity links and CRUD matrix gaps.
- **Access design**: required permissions/media and actor-permission mismatches.
- **State design**: unreachable variants, missing creation paths, state rule violations.
- **Boundary design**: API/system/event-flow inconsistencies and orphaned nodes.

## Main Documents

- [Changelog](./CHANGELOG.md)
- [CLI Reference](./docs/cli-reference.md)
- [Language Reference](./docs/language-reference.md)
- [Incremental Modeling Flow](./docs/incremental-modeling.md)
- [Diagram Sample Review Guide](./docs/diagram-sample-review.md)
- [State Pattern Derivation](./docs/state-derivation.md)
- [RDRA-ish Interpretation](./docs/rdra-ish-interpretation.md)

## Samples

- `samples/ec-site`: compact end-to-end sample
- `samples/clinic-ops`: larger model with APIs, events, and access constraints
- `samples/personal-info`: personal data management sample

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
  rdra-ish-emit/     PlantUML/Mermaid/CSV/state emitters
  rdra-ish-render/   plantuml.jar wrapper
  rdra-ish-cli/      rdra-ish command
samples/
  clinic-ops/
  ec-site/
  personal-info/
```

## License

MIT
