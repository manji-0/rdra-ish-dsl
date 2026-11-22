# Frontends audit: rdra-ish-syntax, rdra-ish-cli, rdra-ish-lsp

Goal: a behavior-preserving refactoring plan. Objectives: (1) efficiency and parallelism, (2) algebraic modeling, (3) file split, (4) reviewability, (5) workarounds, (6) comments.

Method: read-only. I read every non-test source file in scope at least in outline and read the hot paths in full. I ran no cargo commands. Every efficiency claim is labelled. "Inferred" means the call count or complexity comes from reading the code and the wall time is unmeasured.

Findings are ranked by value over risk. "Independent" means the finding can land as its own PR in any order.

---

## Answers to the three scoped questions

**removed.rs: keep it.** It is a deliberate migration-diagnostic table, not legacy code. `LEGACY_FORMS` (removed.rs:72) is the single list that drives three things: removed-keyword errors (`migration_hint`, used at parser.rs:917 and parser.rs:950), predicate rewrites (`legacy_predicate`, parser.rs:1032), and the deprecation pipeline (`Parsed.deprecations` → core `RdraError::DeprecatedSyntax` → `check --deny-deprecated`). About 430 of its 906 lines are tests that pin each message. Three notes, none worth a PR on its own:
- No row is `Status::Deprecated` today. The deprecation pipeline is therefore unreachable in production, and the only test of it pushes a synthetic `Deprecation` (core resolver/tests.rs:8). The module doc names it as the path for the next deprecation, so it is designed capacity, not dead code. Leave it.
- `Status::Removed { since }` is never read at runtime. It is documentation in data form. Leave it.
- Two `Detect::Grammar` rows only document a form whose detection is elsewhere: `removed_when` in removed.rs:239, and the `acceptance` clause in metadata.rs:186. That is acceptable because neither one fits the rewrite-function shape (`when` drops the item and needs `src`).
- Dropping the 0.3.0-era rows is a product decision, not a refactor. Those keywords would then fall to generic parse errors. `tests/removed_cli.rs` and the removed.rs tests pin them.

**CLI business logic: mostly in the right place.** One real misplacement is the TLC adapter (F2). `prove.rs` only formats output (the proof itself is `core::lifecycle::prove`). `lifecycle_gate.rs` is CLI policy over emitters. `verify.rs` orchestrates subprocesses, and that does not belong in core. `property_cone.rs`, `tlc_output.rs`, `tlc_replay.rs` and `tlc_env.rs` are pure library code. A test compiles them by `#[path]` include. They belong in a library (F2). `list_output/state_findings.rs` sits under the wrong parent module (F10).

**CLI loading parallelism and LSP duplication.** The CLI and the LSP share one loading path (`core::load_project`; LSP backend.rs:69). The duplication is inside the CLI: 8 copies of load plus reject (F6), 2 copies of TLA output-path resolution (F5), and many redundant lifecycle explorations (F1). Parsing is serial in core's resolver BFS (F14, which crosses into core's scope).

---

## Ranked findings

### F1. The lifecycle BFS runs again for every gate, sometimes twice for one output. Objectives 1 and 5.
- `lifecycle_gate.rs:144-150`: `Output::gate` calls `gate(...)`. For `Reads::States` that call runs `reject_state_errors`, which runs `state_consistency_findings`, which runs `derive_states_built(model, &Scope::ALL, Bounds::STATES, Caps::STATES)` (state_findings.rs:53). The next lines then run the identical exploration again:
  ```rust
  gate(program, model, self.reads())?;
  ...
  Output::States(emitter) => {
      let (report, _) = derive_states_built(model, &Scope::ALL, Bounds::STATES, Caps::STATES);
  ```
- `Reads::Declared` and `Reads::System` run a full BFS only to print a one-line count (`note_state_errors`, state_findings.rs:694). Every `list`, `csv`, declared `export`, declared `diagram` and `export --kind tla` pays for that BFS.
- `diagram --all` (diagram.rs:323) gates each of 13 kinds separately: 12 notes plus 2 for `state`, so 14 explorations of the same model at the same bounds.
- Proposed change: explore once per command. Add a small `Lifecycle` value that holds `OnceCell<(StateReport, findings)>`, or compute it once at the top of the command and pass it in. `gate` and `Output::gate` read from it. `Gated::States` reuses the same report for `Explored::from_report`. Keep the notes and errors byte-identical.
- Optional second step, which changes behavior and is your call: skip the note for `Reads::Declared` outputs, and they do no BFS at all.
- Efficiency: the call counts are a fact from the code. The time saved is inferred to be large on big models (core reduce.rs:498 says the domain samples "explore millions of states") and is unmeasured. Measure it with `time rdra-ish diagram --all --out /tmp/d <domain sample>` before and after.
- Diff: about +40/-25.
- Risk: the order of the stderr notes. Pinned by `tests/export_gate.rs` ("the lifecycle has"), `tests/diagram_*.rs`, `tests/list_kinds.rs`, and `lifecycle_gate::tests`.
- Independent: yes. It touches lifecycle_gate.rs, state_findings.rs and diagram.rs `write_all`.

### F2. The TLC adapter modules are compiled into a test by path. Objectives 3 and 5.
- `tests/engine_equivalence.rs:30-41` contains `#[path = "../src/property_cone.rs"] #[allow(dead_code)] mod property_cone;`, and the same for `tlc_env`, `tlc_output` and `tlc_replay`. The modules carry workaround comments: "Std only: `tests/engine_equivalence.rs` includes this file by path" (tlc_output.rs:6, tlc_env.rs:2, property_cone.rs:2). They also duplicate `#[allow(dead_code)]` across two compilation units.
- Proposed change: move the four files into a library. The smallest home is `rdra-ish-emit/src/tla/tlc/` (`emit` already owns `tla::print`, `TlaNames` and `PrintMode`, and it depends on core). Another option is a `lib.rs` target in rdra-ish-cli. The test then uses normal `use` paths, and the "std only" constraint and its comments go away.
- Diff: file moves plus about 20 changed import lines.
- Risk: compile-only. `engine_equivalence.rs` and the in-file tests pin behavior.
- Independent: yes. Do it before F3 if both are planned, because F3 edits verify.rs imports.

### F3. `verify::Ran` encodes "undecided" as `Ok` plus two side fields. Objective 2.
- verify.rs:117-129 defines `outcome: Outcome`, `undecided: Option<String>` and `capped: bool`. An undecided property gets `outcome = Outcome::Ok` (verify.rs:366-370). `capped` only means something when `undecided.is_some()`. `run_undecided_wholes` then marks a phase change by clearing every run's field (verify.rs:892-894):
  ```rust
  for r in &mut *ran { r.undecided = None; }
  ```
  `Verdict::NeedsWfNext => unreachable!("the WF run answers it")` (verify.rs:372) is a second illegal state that the types allow.
- Proposed change: replace the three fields with `enum Decision { Decided(Outcome), Undecided { why: String, capped: bool } }`. The cone phase returns `Decision`. The whole-model phase maps `Undecided` to its result, so nothing gets cleared. `Ran::line` (the "not decided" text), `undecided_lines`, and the outcome fold in `run_tlc` (verify.rs:760) match on the enum. For `NeedsWfNext`, split `PropertyCone::verdict` into `after_plain(plain) -> Either<Verdict, NeedsWf>` and `after_wf(plain, wf) -> Verdict`, which removes the `unreachable!`.
- Diff: about ±70 in verify.rs and ±20 in property_cone.rs.
- Risk: verify output lines and exit codes. Pinned by `tests/verify_cli.rs`, `verify::hint_tests`, and the `property_cone` unit tests. TLC-gated tests skip without `tlc`, so run with `RDRA_REQUIRE_TLC=1`.
- Independent: yes. It conflicts textually with F2 and F15.

### F4. Two different PATH lookups. Objective 5.
- verify.rs:1161 shells out to the `which` binary:
  ```rust
  fn which(cmd: &str) -> Result<PathBuf> { let output = Command::new("which").arg(cmd)...
  ```
  prove.rs:47-52 walks `PATH` in-process with `std::env::split_paths`. On a system without `which` (minimal containers, Windows), `verify` reports "TLC not found" even when `tlc` is on PATH.
- Proposed change: add one `fn find_on_path(name) -> Option<PathBuf>` (the prove.rs version), used by `Tlc::find` and `find_z3`. Delete `which`.
- Diff: about -15.
- Risk: `which` also checks the executable bit and `is_file` does not, so a non-executable `tlc` file earlier on PATH would now be picked. Add an exec-bit check on unix to keep parity. No test pins `which`. The "not found" message is pinned by `tests/verify_cli.rs` and `tests/prove_cli.rs` if they assert it.
- Independent: yes.

### F5. TLA output-path resolution is written twice, and main.rs holds a 90-line command body. Objectives 4 and 5.
- main.rs:262-300 (the Export arm) and verify.rs:151-176 (`out_dir`) both map `-o X.tla` to (parent dir, stem `X`) and default to `RdraSpec`. main.rs also has redundant branches:
  ```rust
  let dir = if out.exists() && out.is_dir() { out.clone() }
            else if out.extension().is_none() { out.clone() }
            else { out.parent()... };
  ```
  (`is_dir()` already implies `exists()`, and both branches return `out`.) The `if artifact.is_none() {...} else if let Some(...) = artifact` (main.rs:262, 324) is a `match` written as two tests.
- Proposed change: add `enum TlaOut { File { dir, stem }, Dir(PathBuf) }` with `TlaOut::parse(&Path)` in export.rs, used by both commands. Move the Export arm to `export::run_export(...)`, like every other command that has a `run_*`. Turn the artifact split into `match export_output(..) { Export::Artifact(..) => .., Export::Tla => .. }`.
- Diff: main.rs about -90, export.rs about +60, verify.rs about -20.
- Risk: written file paths. Pinned by `tests/export_gate.rs` (RdraSpec.cfg, StateModel_*.cfg) and `tests/verify_cli.rs`. Keep the `-o foo.cfg` case, which currently writes `foo.tla`/`foo.cfg` through the final else branch.
- Independent: yes.

### F6. Load-and-reject is copied 8 times. Objective 4.
- The same 7-line block appears in codegen_cmd.rs:22, csv_cmd.rs:22, diagram.rs:66, states.rs:25, next.rs:20, and main.rs:182, 252 and 371:
  ```rust
  let Project { program, model, diags, .. } = load_project(inputs, None)?;
  reject_model_errors(&program, &diags)?;
  ```
- Proposed change: add `load::load_clean(inputs) -> Result<(ResolvedProgram, SemanticModel)>`. `check` and `lint` keep their own paths because they print instead of rejecting.
- Diff: about -45.
- Risk: none. Same calls in the same order.
- Independent: yes.

### F7. The deadlock exemption matches a string prefix. Objective 2.
- state_findings.rs:670-672:
  ```rust
  reject_errors_where(program, model, |finding| !finding.name.starts_with("Deadlock_"))
  ```
  `Finding` has already lost the `StateDiag`, so the filter parses the name back out.
- Proposed change: filter on the diagnostic before rendering (`matches!(diag, StateDiag::Deadlock { .. })`). Either carry a `kind` or `is_deadlock` on `Finding`, or have `state_findings` return `(StateDiag ref, Finding)` pairs.
- Diff: about ±15.
- Risk: a non-deadlock diagnostic whose name starts with `Deadlock_` would change classification. None exists today (inferred from `StateDiag::name`). Pinned by the `diagram --kind state` tests in `tests/diagram_*.rs`.
- Independent: yes. It combines naturally with F1.

### F8. `diagram --all` swallows every render error and detects emptiness by scanning its own text. Objective 5.
- diagram.rs:335-345:
  ```rust
  Err(_) => { empty += 1; return Ok(()); }   // "A sequence diagram of a BUC without use cases."
  ...
  if lines.iter().any(|l| l.starts_with("note: nothing to draw")) {
  ```
  `findings` (diagram.rs:406) detects "empty" by matching the emitted `%% kinds: (none)` line, and `write_all` then matches the prefix of the message `findings` itself wrote.
- Proposed change: have `findings` return `enum Finding { Empty, TooLarge(String) }`, or check emptiness once with a typed helper. Replace `Err(_)` with a match on the specific `EmitError` variant the sequence emitter returns for a BUC with no use cases, and propagate every other error.
- Diff: about ±30.
- Risk: today, any other emitter error during `--all` is silently counted as "left out". After the change it fails the command. That is a behavior change, but it fixes a bug and only applies to errors that should not happen. Pinned by `tests/diagram_*.rs` (`--all` counts).
- Independent: yes.

### F9. The `diagram` argument struct is defined twice, and kind names are hand-mapped. Objective 4.
- main.rs:132-160 destructures `Commands::Diagram { 13 fields }` only to rebuild `DiagramRequest { 12 fields }` (diagram.rs:28). `kind_name` (diagram.rs:586-603) re-types the clap `ValueEnum` names.
- Proposed change: declare `Commands::Diagram(DiagramArgs)` with `#[derive(Args)]` and pass the struct to `run_diagram(args)`. Replace `kind_name` with `kind.to_possible_value().expect("no skipped variants").get_name()`. `kind_title` and `diagram_output` stay, because they carry real data.
- Diff: about -60.
- Risk: clap help text must stay the same. Pinned by `cli::tests` (cli.rs:613, which matches `Commands::Diagram { .. }`; the test needs updating) and by cli_tests.rs:1056, which builds `DiagramRequest`.
- Independent: yes.

### F10. `state_findings.rs` is not a list formatter. Objective 3.
- `list_output/state_findings.rs` (858 lines) is "State diagnostics as `check` and the generator gates print them, and the one renderer of counterexample traces." Its users are main.rs `check`, verify.rs, states.rs, codegen_cmd.rs, export.rs and lifecycle_gate.rs. `list` itself does not use it. `list_output/mod.rs` re-exports 12 of its items for those users.
- Proposed change: move it to `src/findings.rs`, a top-level CLI module, and drop the re-exports.
- Diff: a pure move plus about 15 import lines.
- Risk: none.
- Independent: yes. Do it before F1 and F7 to keep their diffs small.

### F11. LSP handlers repeat the same 10 to 14 lines. Objectives 4 and 6.
- backend.rs:297-675: each of 15 handlers runs `uri_to_path` → `return Ok(None)`, then `self.state.read().await`, then `state.analysis.as_ref()`, then `open_document` or `byte_offset_at`. There are 44 `async fn`/read-lock lines. The file also has dead leftovers:
  ```rust
  for (ref_source_id, (ref_path, ref_text, ref_ast)) in analysis.program.sources.iter().enumerate() { ... let _ = ref_source_id; }  // backend.rs:357-371
  let _ = doc.ast;                                                                                  // backend.rs:392
  ```
  semantic_tokens.rs:150 has `let _ = kind;` (use `qref.kind_qualifier.is_some()`). In `references`, `path_to_uri(ref_path)` canonicalizes the path for every span instead of once per source.
- Proposed change: add two sync helpers on `ServerState`: `doc(&self, uri) -> Option<(&WorkspaceAnalysis, OpenDocument)>` and `cursor(&self, uri, pos) -> Option<(&WorkspaceAnalysis, OpenDocument, usize)>`. Each handler becomes 3 to 5 lines. Delete the dead lets, and hoist the URI out of the span loop.
- Diff: about -130.
- Risk: none for behavior. Pinned by `workspace_test.rs`, the module unit tests, and `tests/manifest_error.rs`.
- Independent: yes.

### F12. LSP refresh blocks the runtime and can publish stale analysis. Objectives 1 and 5. This changes behavior.
- backend.rs:47-81: `refresh_analysis` runs on every `did_change`. It clones every open buffer (`state.documents.clone()`), re-reads and re-parses every workspace file, and runs `analyze_project` synchronously inside the async handler. It has no generation check. If two refreshes overlap, the slower, earlier one can write `state.analysis` and publish diagnostics last, which leaves old results showing. The race is inferred and depends on how tower-lsp 0.20 runs notifications concurrently, which I did not verify.
- Proposed change: keep an `AtomicU64` generation in `ServerState`. Bump it when a document changes, run load and analyze in `tokio::task::spawn_blocking`, and discard the result if the generation moved. Debouncing is optional.
- Diff: about +30.
- Risk: the timing of diagnostics publication. Pinned by `tests/manifest_error.rs` and `workspace_test.rs`. This is a fix, not a pure refactor, so ship it on its own.
- Independent: yes.

### F13. LSP code lens and document lookup are quadratic and call canonicalize on every request. Objective 1.
- code_lens.rs:25-33 calls `reference_locations` (code_lens.rs:68) for each instance in the file. That walks every item of every source AST and calls `path_to_uri`, which runs `dunce::canonicalize` (a syscall), for every source. The cost is O(instances × workspace items) plus O(instances × files) syscalls per code-lens request. `span_to_range` costs O(offset) per hit (convert.rs:13 rescans from byte 0).
- document.rs:20-32: `open_document` calls `paths_equal`, which canonicalizes both paths for every source on every hover, token and lens request (uri.rs:13).
- Proposed change: (a) when analysis is built, build one `HashMap<SymbolTarget, Vec<Location>>` and a per-source line-start index. Code lens and references read from them. (b) Canonicalize the request path once and compare it to `program.sources` paths, which the resolver already canonicalized (core resolver.rs:55).
- Efficiency: the complexity is inferred from the code. Wall time is unmeasured. It matters for files with hundreds of declarations, because editors re-request code lens after every edit.
- Diff: about +60/-30.
- Risk: the order of reference locations. Pinned by the `code_lens` and `refs` unit tests.
- Independent: yes. Part (b) is a 5-line change.

### F14. Parsing is serial. This is in core, but CLI and LSP latency depend on it. Objective 1.
- core resolver.rs:54-108 reads, then parses, then enqueues imports one file at a time. Every entry file is known up front (`collect_rdra_files`, project.rs:72). Two smaller costs: syntax parser.rs:1065 copies the source into `Ast.source` while `program.sources` keeps another copy, and removed.rs:190 runs `let mut current = pred.clone()` on every predicate call in every parse, even when no legacy row names that predicate.
- Proposed change: parse all entry files in parallel with `std::thread::scope`, so no new dependency, then run the BFS only for imports outside the entry set. Check names before the clone in `legacy_predicate`. `fmt` (fmt_cmd.rs:24) can format files in parallel the same way and print them in order.
- Efficiency: guess. Parse cost has not been profiled against the lifecycle BFS (F1), which probably dominates. Profile `check` on a domain sample first, and do this only if parsing shows up.
- Diff: about +40 in core.
- Risk: diagnostic order, which depends on `SourceId` assignment order. Keep assigning ids in the current order. Pinned by the core resolver tests and every CLI snapshot.
- Independent: yes.

### F15. Building the verify plan is serial, and the BFS and z3 work runs before TLC starts. Objective 1.
- verify.rs:197-246 (`cone_jobs`), 252-292 (`property_jobs`) and export.rs:111-139 (`state_model_specs`) each call `reduced_cone(.., Caps::VERIFY)` per cone, one after another. Core reduce.rs:1725 runs `sys.reduced(&local, caps)`, which explores. Then `state_models` (BFS) and `run_proof` (z3) run before the first TLC process starts (verify.rs:713-726).
- Proposed change: claim all `Modules` names first, which is sequential and keeps the names deterministic, then build the cones with `std::thread::scope` (the same pattern as `run_some`, verify.rs:530). This requires `TransitionSystem: Sync`, which I did not verify. Overlapping the BFS and z3 work with the TLC cone runs would require buffering their output, so it is lower value.
- Efficiency: guess. TLC wall time probably dominates verify.
- Diff: about +30.
- Risk: module naming order, pinned by `tests/verify_cli.rs` and `tests/engine_equivalence.rs`.
- Independent: it conflicts textually with F3.

### F16. `InstanceDecl` claims kind-specific metadata the parser does not enforce. Objective 6 now, objective 2 later.
- ast.rs:274-285 says `/// Non-empty only for \`api\` declarations.` on `api: ApiMetadata`, and makes the same claim for 5 other structs. The parser accepts any clause on any kind (metadata.rs:88 `CLAUSES`, `Metadata::collect`). The formatter writes every group back for every kind (format.rs:307-312). An `actor X "x" method "GET"` therefore parses and round-trips. The comments are false.
- Proposed change, first step: reword the comments to say "the clauses of this group; the parser accepts them on any kind". Diff about ±6, no risk.
- Proposed change, second step (needs your call): `enum KindMetadata { Requirement(..), Adr(..), Api(..), Nfr(..), Field(..), UseCase(..), None }`, keyed by `Kind`. That rejects cross-kind clauses, which changes behavior, and it reaches into every core consumer of `inst.requirement`, `.api` and the rest. That is a large blast radius outside this scope.
- Independent: yes.

### F17. The metadata printer duplicates the clause table by hand. Objective 4.
- format.rs:369-601 is 230 lines with one `if let Some(value) = &instance.X.y { lines.push(format!("  kw {}", meta_value(instance, "kw", value))) }` per field. metadata.rs:88 already holds the keyword table.
- Proposed change: add a printer table `&[(&str /*keyword*/, fn(&InstanceDecl) -> Vec<&str>)]` in today's output order, so `format_*_metadata` becomes one loop. `required` (a bool) stays a special case.
- Diff: about -150.
- Risk: canonical clause order and `bare_values` quoting. Pinned by `format/tests.rs` (389 lines) and the parser round-trip tests. Generate the table from today's order, not from `CLAUSES` order, which differs (for example `guard` comes before `postcondition` in fmt).
- Independent: yes.

### F18. `(InvKind, InvText)` is a pair of enums that must agree. Objective 2. The type is in core.
- state_findings.rs:712-779 matches `(kind, text)` with `_ => return None`. verify.rs:641-642 then has to assert:
  ```rust
  let message = render_violation(program, model, &diag, TraceSource::Tlc).expect("explain returns a violation");
  ```
- Proposed change: in core, merge them into one `enum InvBody { Forbidden(Conditions), Invariant { guards, requireds }, ... }`. The CLI match becomes exhaustive, `render_violation` returns `String` for a violation, and the `expect` goes away.
- Diff: about ±80 across core and CLI.
- Risk: none for messages if the arms keep their strings. Pinned by `cli_tests.rs` (state_diag_message tests) and the core lifecycle tests.
- Independent: yes, but it needs an owner on the core side.

### F19. Small workarounds and leftovers. Objectives 5 and 6. Batch them into one PR.
- csv_cmd.rs:71 and main.rs:333 call `ext.trim_start_matches("*.")`. No extension in `export_output` or csv_cmd starts with `*.`, so the call does nothing. Delete it.
- codegen_cmd.rs:40 calls `line.strip_prefix("bounds: ")` on `BoundsLine`'s `Display` output, which formats a string and parses it back. Add a `BoundsLine` method that returns the body.
- predicates.rs:5-35: `format_predicate_signature` and `predicate_signature_parameters` repeat the same filter and differ only in separator (`"|"` vs `" | "`). Write one helper that takes the separator.
- rename.rs:83 uses `let _ = rename_span_at_offset(..).ok_or(RenameError::NoSymbol)?;` as a guard. Write `if rename_span_at_offset(..).is_none() { return Err(..) }` so the intent is readable.
- Diff: about -25. Risk: none. Pinned by the existing unit tests.

---

## Leave as is
- `lifecycle_gate.rs:38-90`: the `ListKind::reads` and `CsvKind::reads` matches return `Declared` for every arm. That looks redundant, but the module doc says it is deliberate: a new kind must declare what it reads before it compiles.
- `cli.rs` `WithRemoved` and the `REMOVED_*` tables: a deliberate migration UX, the same policy as removed.rs.
- `cli::VerifyBackend` and `OutputFormat` have one variant each. The flags exist so that removed values get a hint. Fine.
- `tlc_output.rs` (TLC log parser), `run_some` (verify.rs:530, a small scoped pool), and `run_whole` (polls on a budget) are clear and tested.
- `prove.rs`: presentation only, so it belongs in the CLI.
- `parser.rs` layout, `metadata.rs` (table-driven), and `parse_error.rs`.
- `cli_tests.rs` (1441 lines) and `parser/tests.rs` (1322 lines) are large but are tests. Splitting them by subject would help a reviewer only slightly. Not worth a PR on its own.
- The CLI and the LSP have no duplicate loading path. Both go through `core::load_project`.

## Suggested order
F10 → F6 → F1 + F7 → F2 → F3 → F5 → F9 → F11 → F19 → F4 → F8 → F17 → F13(b) → F16 comments → (F12, F13a, F14, F15, F18 need measurement or a core owner).
