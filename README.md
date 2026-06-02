# rdra-ish-dsl

`rdra-ish-dsl` is a CLI tool and DSL compiler for reviewing requirements models as code.

It lets you model actors, use cases, APIs, screens, entities, and relationships, then:
- type-check the model (`check`)
- generate diagrams (`diagram`)
- export review CSVs (`csv`)
- derive reachable entity states (`states`)

## Installation

```sh
uv tool install rdra-ish-dsl
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

- [CLI Reference](./docs/cli-reference.md)
- [Language Reference](./docs/language-reference.md)
- [Incremental Modeling Flow](./docs/incremental-modeling.md)
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
