# rdra-ish

`rdra-ish` keeps requirements and design in one typed model, from the business value a
system serves down to its APIs, data and state transitions. You write the model in a small
DSL. The CLI type-checks it, finds the gaps between layers, verifies the lifecycle, and
projects the model into diagrams, review tables, exports and domain code.

## Install

| From | Command | Gives |
|---|---|---|
| PyPI | `uv tool install rdra-ish` | `rdra-ish`, whose `rdra-ish lsp` runs the language server |
| Nix (flakes) | `nix profile install github:manji-0/rdra-ish-dsl` | `rdra-ish`, `rdra-ish-lsp`, and the `tlc` and `z3` that `verify` runs |
| Cargo | `cargo install --locked --git https://github.com/manji-0/rdra-ish-dsl rdra-ish-cli rdra-ish-lsp` | `rdra-ish`, `rdra-ish-lsp` |

Each [GitHub release](https://github.com/manji-0/rdra-ish-dsl/releases) also has `rdra-ish`
binaries for Linux, macOS and Windows. For a Nix install of a release after v2.3.0, add
`/<tag>` to the URL. `nix run github:manji-0/rdra-ish-dsl -- --help` runs without
installing. A `tlc` or `z3` already on `PATH` takes precedence over the ones Nix brings.

### Claude Code plugin

<!-- constrained-by ./docs/cli-reference.md#lsp -->
The repository is also a Claude Code plugin marketplace. The `rdra-ish` plugin brings the
[agent skills](./skills/README.md) and the language server for `.rdra` files:

```text
/plugin marketplace add manji-0/rdra-ish-dsl
/plugin install rdra-ish@rdra-ish
```

Claude Code 2.1.275 or later also takes one step,
`/plugin install rdra-ish --marketplace manji-0/rdra-ish-dsl`. The plugin does not install
the binary: the language server runs `rdra-ish lsp`, so put `rdra-ish` on `PATH` with the
PyPI or Nix install above. The skills work without it, but they run `rdra-ish` too.

Claude Code clones the whole repository for the marketplace, about 27 MB. To clone only what
the plugin needs, about 1.3 MB, add it with a sparse checkout instead:
`/plugin marketplace add manji-0/rdra-ish-dsl --sparse .claude-plugin plugins`.
The install itself copies only the plugin (`plugins/rdra-ish/`: the skills, `.lsp.json` and
the manifest) into Claude Code's plugin cache.

## Five-minute tour

```sh
# Type-check, and see the next refinement stage of each BUC
rdra-ish check samples/ec-site
rdra-ish next samples/ec-site

# Diagrams for review, as Mermaid text
rdra-ish diagram samples/ec-site --kind usecase --buc BucOrder
rdra-ish diagram samples/clinic-ops --all --out diagrams

# Reachable states, and rules as a table
rdra-ish states samples/clinic-ops --entity Appointment
rdra-ish list samples/clinic-ops --kind rule --format markdown

# Verify every rule and property; then cross-check with TLC and prove with z3
# (needs tlc and z3 on PATH, which the Nix install brings)
rdra-ish verify samples/formal-verification/order.rdra
rdra-ish verify samples/formal-verification/order.rdra --with-tlc --prove

# Domain types for a build
rdra-ish codegen samples/ec-site --lang ts -o domain.ts
```

## What it does

| Feature | What you get | Read |
|---|---|---|
| `check`, `lint`, `next` | Type errors, unresolved references, the gaps between layers, and the refinement stage of each BUC with its open questions | [CLI reference](./docs/cli-reference.md#check) |
| `diagram`, `list`, `csv` | The RDRA 3.0 diagrams as Mermaid text, scoped by BUC, use case, actor or entity; CRUD matrix, permission audit, rule tables | [`diagram`](./docs/cli-reference.md#diagram) |
| `export` | OpenAPI, AsyncAPI, DBML, SQL DDL, JSON Schema, TLA+ | [`export`](./docs/cli-reference.md#export) |
| `states` | The reachable states of each entity and the state rules they break | [State derivation](./docs/state-derivation.md) |
| `verify` | Every invariant, `after` assertion and temporal property, decided by a native checker with no JVM; `--with-tlc` cross-checks it with TLC, and `--prove` (experimental) proves state rules for any number of rows with z3 | [Formal verification](./docs/formal-verification.md) |
| `verify --symmetry always` | Explores a cone up to instance symmetry, so larger instance counts fit in memory (experimental) | [Instance symmetry](./docs/cli-reference.md#instance-symmetry) |
| `codegen` | Domain code in the Kamae style for TypeScript, Rust and Go: states, rows, transition functions, contracts and a conformance harness. The TypeScript types are covered by the compatibility contract; the rest is experimental | [Code Generation](./docs/codegen.md), [`codegen`](./docs/cli-reference.md#codegen), [design](./docs/design/algebraic-codegen.md#generated-code-style) |
| `check --code` | Checks that the service code is what the model generates, compiles against the contracts, and passes the conformance test (experimental) | [`check --code`](./docs/cli-reference.md#check---code), [Code Generation](./docs/codegen.md#what-the-output-guarantees) |
| `rdra-ish-lsp` | Language server: diagnostics, completion, hover, definitions, references, rename, formatting, code lenses and semantic colors, and `check`'s lifecycle findings from a background run | [VS Code extension](./editors/vscode/) |
| Agent skills | Skills for writing, reviewing, diagramming and verifying a model, and for implementing a service against its generated contracts | [`skills/README.md`](./skills/README.md) |

The VS Code extension in [`editors/vscode`](./editors/vscode/) runs `rdra-ish-lsp` from
`PATH`, or from the `rdra-ish.languageServerPath` setting. Build it with `npm run package`
there. The [Claude Code plugin](#claude-code-plugin) runs the same server as
`rdra-ish lsp`.

## Where the method comes from

rdra-ish joins three sources in one model. Each part keeps the names its source uses.

| Part of the model | Source | Elements |
|---|---|---|
| Requirements and business, above the system | RDRA (Zenji Kanzaki) | `requirement`, `goal`, `business`, `buc`, `flow`, `step`, `variation`, `condition`, `event`, `actor`, `extsystem` |
| From use case to design, below the system boundary | ICONIX robustness analysis | `usecase`, `screen`, `api`, `entity`, `field` |
| Lifecycle and verification | rdra-ish's own | lifecycles, state rules, temporal properties |

From RDRA comes the central idea: one graph of typed elements, where each layer gives the
reason for the next. Below the system boundary, ICONIX reads each use case as boundary
(`screen`, `extsystem`, `timer`), control (`api`) and entity (`entity`) objects, which gives
the robustness, sequence and ER diagrams. rdra-ish stops before object-oriented design. It
ends at the APIs, the data and the states. The name stands for RDRA-inspired
Implementation and System Heuristics. [RDRA-ish Interpretation](./docs/rdra-ish-interpretation.md)
says how it reads BUC, business flow and use case, and [Vision](./docs/vision.md) records
the design decisions.

## Recommended Modeling Loop

Refine the model in small stages instead of filling every layer at once. After each stage,
run `rdra-ish check`, and review only the concern you just added with a diagram or a table.
`rdra-ish next` says which stage each BUC has reached and what blocks the next one.
[Incremental Modeling Flow](./docs/incremental-modeling.md) gives the stages, and
[samples/incremental-order](./samples/incremental-order/) walks through seven of them.

The diagrams and tables are made to go into a PRD and a design document.
[PRD and Design Document Guide](./skills/rdra-ish-diagram/references/documents.md) says
which goes where; [`prd.md`](./samples/clinic-ops/prd.md) and
[`design-doc.md`](./samples/clinic-ops/design-doc.md) of clinic-ops are generated that way.

## Documentation

- [Incremental Modeling Flow](./docs/incremental-modeling.md): start here
- [Language Reference](./docs/language-reference.md) and [CLI Reference](./docs/cli-reference.md)
- [State Derivation](./docs/state-derivation.md) and [Formal Verification](./docs/formal-verification.md)
- [Code Generation](./docs/codegen.md): what `codegen` writes and what `check --code` checks
- [Compatibility](./docs/compatibility.md): what each release keeps stable
- [Diagram Sample Review Guide](./docs/diagram-sample-review.md)
- [Changelog](./CHANGELOG.md)

## Samples

| Sample | Shows |
|---|---|
| `samples/incremental-order` | The seven-step refinement walkthrough |
| `samples/ec-site` | A compact end-to-end model |
| `samples/clinic-ops` | APIs, events and access constraints, with a generated PRD and design document |
| `samples/patent-ops`, `samples/food-permit-ops`, `samples/insurance-claim-ops` | Three Japanese business domains modeled from primary sources |
| `samples/oidc-auth` | An OpenID Provider with every OpenID Connect authentication scenario ([scenarios](./samples/oidc-auth/README.md#scenarios)) |
| `samples/personal-info`, `samples/api-contract` | Personal data handling; API contract metadata and the OpenAPI export |
| `samples/formal-verification`, `samples/formal-verification-fail` | Small `verify` examples, one file each |

## Versions

rdra-ish follows semantic versioning. [Compatibility](./docs/compatibility.md) lists what a
release keeps: a model that loads keeps loading, and `check` and `verify` give it the same
verdict within the same printed bounds. `main` gets each change first and may break those
promises between releases; the `Unreleased` section of the [changelog](./CHANGELOG.md)
lists what changed. A model written for 0.4.x upgrades with
`python3 scripts/migrate-syntax-v3.py <model dir>` ([v1.0.0 notes](./CHANGELOG.md#v100---2026-11-03)).

## Development

```sh
nix develop          # Rust, maturin, uv, Java, TLC, z3, Node.js, TypeScript, Go, pre-commit
cargo test --all
nix build            # rdra-ish and rdra-ish-lsp into ./result
```

CI runs every step in the same `nix develop` shell. `pre-commit install` sets up a hook
that runs the whitespace checks, `cargo fmt`, `cargo clippy` and the tests of the crates a
commit touches.

The files in `samples/formal-verification/` are symlinks into
`skills/rdra-ish-verify/samples/`. On Windows, clone with `git clone -c core.symlinks=true`.

A release is a `v*` tag push: it builds the binaries, publishes the GitHub release, and then
publishes to PyPI.

## Project layout

```text
crates/
  rdra-ish-syntax/   Lexer, parser, AST
  rdra-ish-core/     Semantic model, type checks, lifecycle, native checker, prover
  rdra-ish-emit/     Diagrams, CSV, exports, codegen
  rdra-ish-cli/      The rdra-ish command
  rdra-ish-lsp/      Language server
editors/vscode/      VS Code extension
docs/                Language, CLI, verification, compatibility, design notes
skills/              Agent skills
samples/             Example models
scripts/             Migration scripts and CI checks
```

## License

MIT
