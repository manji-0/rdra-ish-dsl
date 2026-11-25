---
name: rdra-ish-implement
description: >-
  Implement a service in Rust, TypeScript, or Go from an RDRA-ish model: name the
  files in a [code.rust], [code.ts], or [code.go] table of rdra.toml, generate the domain types, state
  types, use case contracts, probe, and conformance traces with rdra-ish codegen,
  write the service against those contracts, and hold it to the model with
  rdra-ish check --code (fresh, build, run) and the obligation ledger. Use when
  the user asks to implement, scaffold, or generate service code from a .rdra
  model, about codegen output or its obligations, about a CodeMissingUseCase,
  CodeConformance, or CodeObligationUnowned finding, or about keeping code in
  sync with a changed model.
license: MIT
---

## When to use

Write or change service code that a model specifies. `codegen` prints the model's data
layer and use case contracts; you write the service that stores rows and calls them;
`check --code` proves the service still has what the model requires. `codegen` and
`check --code` are experimental, and their output may change in any release.

## Steps

1. Make the model pass `rdra-ish check model/`. `codegen` refuses a model `check` rejects.
   Run `rdra-ish verify model/` for rules that need two rows.
2. Name the files in `model/rdra.toml`, paths relative to it. Only `domain` is required.
   `probe` and `service` enable `build`; `traces` adds the conformance test that `run`
   runs (in Rust it also needs `new`). Go, TypeScript, and every key are in
   `references/contracts.md`.

   ```toml
   [model]
   name = "shop"

   [code.rust]
   domain = "../svc/src/domain.rs"
   traces = "../svc/tests/traces.txt"
   probe = "../svc/tests/rdra_contract.rs"
   service = "svc::service::Service"
   module = "svc::domain"
   new = "svc::service::Service::new()"
   ```

3. Run `rdra-ish codegen model --code`. It writes every file the table names and prints
   `wrote <path>` for each. Never edit a generated file; regenerate it.
4. Read the generated header: the **obligations** (what `check` assumes and no type
   enforces), the **candidate rules**, and the use cases it printed no function for.
   `rdra-ish codegen model --format json` gives the same contract as data.
5. Write the service against the contracts (`references/contracts.md`): implement each
   `<UseCase>UseCase` and the `ConformancePort`, load rows through `fromRow`, call the
   dispatcher, and store each row in one transaction only where it still is what the
   result expects.
6. Give each obligation an owner: a line `// rdra: obligation <name> [file]` in the code
   that keeps it, or `<name> = "<file>"` under `[code.obligations]`.
7. Run `rdra-ish check model --code=build`, then `rdra-ish check model --code=run`. Fix
   each finding with `references/code-check.md`. The last line is
   `OK: code: what the model requires, at --code=<level>` or
   `FAIL: code: <n> error(s) at --code=<level>`; paste it as printed.
8. After a model change, run `rdra-ish codegen model --code` again and repeat step 7. In
   CI, `rdra-ish check model --code` (`fresh`, no compiler) fails when a generated file is
   stale. The editor's LSP flags a stale file on its table's line of `rdra.toml`, and runs
   no build.

`--code` needs the table:

```text
$ rdra-ish check ../rdra-ish-verify/samples/order.rdra --code
Error: --code needs an rdra.toml with a [code.rust], [code.go] or [code.ts] table; this model has no rdra.toml
```

## References

- `references/contracts.md`: the `[code.<lang>]` keys, the generated shapes per language,
  and how a service implements a use case.
- `references/code-check.md`: the levels of `check --code`, its findings, and the
  obligation ledger.
- `../rdra-ish-verify/SKILL.md`: verify the model before building on it.
