# Evaluation (roadmap T4, T6, T7, T8, T10, T11)

Every table here is the output of a script on a data file in [data/](./data/). The commands are in
[artifact.md](../artifact.md). Main at `9e4a123`. Machine: a 4-core x86_64 Linux VM, 15 GiB of
memory, kernel 6.18. Build: `cargo build --release -p rdra-ish-cli --features perf`. Each run is one
`verify` process at `RDRA_JOBS=1` (except the determinism table), with `--memory 12GiB` and a
timeout. CPU is the process's user CPU and memory its peak RSS, both from `wait4`. States are the
sum over the run's BFS runs (`perf-run` lines), and "largest run" is the largest of them.

| Page | Question | Data |
|---|---|---|
| [ablation.md](./ablation.md) | T6: what does each of cone, sub-lifecycle reduction and symmetry buy? | `data/ablation.tsv`, `data/whole.tsv` |
| [scaling.md](./scaling.md) | T7: states, CPU and memory against the instance count, with and without symmetry | `data/scaling.tsv` |
| [minimal.md](./minimal.md) | T4: the least instance count of each known counterexample | `data/minimal.tsv` |
| [determinism.md](./determinism.md) | T10: is the output the same across thread counts and runs? | `data/determinism.tsv` |
| [tlc-baseline.md](./tlc-baseline.md) | T8: agreement with TLC, from CI logs | `data/tlc-ci-lines.txt` |

## Threats that apply to every table

- **One machine, one run.** CPU times are from a single run on a VM. The verdict and state columns
  are deterministic (see [determinism.md](./determinism.md)); the CPU and memory columns are not.
  Instructions retired were not available on this VM.
- **Background load.** The whole-model runs (`paper_whole`) and the ablation ran at the same time on
  separate cores. Each is single-threaded, and user CPU is per process.
- **Models written by the authors** (T9). See Paper 1, threat T1.

## T11: the TLC-side mutation run

**Result: a kill rate of 80.4% on a systematic sample** (37 of 46 viable mutants; 95% Wilson
interval 66.8% to 89.3%). The sample is one mutant in 12 of the 569 that `.cargo/mutants-tlc.toml`
lists: cargo-mutants' round-robin shards 0/24 and 12/24, which together are shard 0/12. It ran
locally with cargo-mutants 27.1.0 (the CI version), TLC 1.7.4 and z3 5.4.1 from nixpkgs, and the CI
job's test command and timeouts (`--baseline skip --timeout 900`). Main at `9e4a123`. Of the 48
mutants: 37 caught, 9 missed, 2 unviable, 0 timeouts. Data:
[data/mutants-tlc-sample.txt](./data/mutants-tlc-sample.txt).

The nine survivors. Each marks code that no test in the mutant's test command (the corpus under
TLC and z3, the TLC reader's unit tests, five `verify_cli` tests) distinguishes from its mutant.
Whether each is an equivalent mutant or a missing test needs a look at each one.

| File | Mutant | Area |
|---|---|---|
| `tla/pred.rs:137` | delete the `CmpOp::Ne` arm of `Ctx::cmp` | printing of `!=` in predicates |
| `tla/mod.rs:133` | `==` to `!=` in `render_value` | printing of values |
| `tla/module.rs:917` | `==` to `!=` in `from_guarded` | printing of guarded effects |
| `tla/module.rs:974` | `bindings` returns `[[1]]` | the bindings table of the module |
| `tla/names.rs:100` | `+=` to `*=` in `Taken::claim` | name disambiguation |
| `tla/names.rs:301` | `rise_table` returns `Some("xyzzy")` | the name of the rise table |
| `tla/names.rs:439` | `slot_keys` returns `[]` | slot key names |
| `tlc_output.rs:387` | a match guard to `false` in `parse_block` | reading TLC's trace blocks |
| `tlc_output.rs:442` | `+=` to `*=` in `split_top` | splitting nested TLC values |

The full run did not complete in CI (below). At about 210 s per mutant, the 569 mutants need about
33 hours on one machine.

### The attempt in CI

The `Mutants` workflow (`.github/workflows/mutants.yml`) with `config=tlc` mutates the TLA+ printer
(`crates/rdra-ish-emit/src/tla/`), the comparison (`verify/compare.rs`) and the TLC output reader
(`tlc_output.rs`, `tlc_replay.rs`). It tests each mutant with the engine corpus under TLC and z3.
No run of it had completed before this work: the one earlier run (46052978676 on branch
`mutants-tlc`, 2026-11-27) was cancelled. This work started run
[46128666288](https://github.com/manji-0/rdra-ish-dsl/actions/runs/46128666288) on main, with four
shards, on 2026-11-28.

**Result in CI: no kill rate.** All four shards were lost before they uploaded results ("The runner has
received a shutdown signal"), after 10, 26, about 58 and about 62 minutes. The summary job found 0
artifacts. What the logs show:

- Each shard lists about 143 mutants (shard 0: 143; shard 3: 142), so about 570 in all.
- Each mutant's test (the engine corpus under TLC and z3, the TLC reader's unit tests and five
  `verify_cli` tests) takes about 280 s. One shard would need about 11 hours, while the workflow
  allows 330 minutes per shard step. The whole run is about 45 runner-hours.
- Shard 0 tested two mutants before it was lost. Both were **missed**:
  `tlc_output.rs:90:58` (`||` to `&&` in `TlcRun::parse`) and `tlc_output.rs:184:52` (`&&` to `||` in
  `violation_kind`). Two survivors say nothing about a rate, but both are worth a test of their own.
- The `tlc` configuration runs without an address-space cap, because the JVM cannot start under one.
  A mutant that makes the checker allocate without bound would exhaust the runner, which matches
  the shutdowns. The logs do not show the cause.

**What completing T11 needs** (a workflow change and an owner's decision on the CI cost): about 24
shards or a narrower `re` per run; a per-mutant memory cap that the JVM tolerates (a cgroup limit
such as `systemd-run --scope -p MemoryMax=`, or `prlimit` on the `rdra-ish` process with
`RDRA_TLC_HEAP` set); and a shorter test command per mutant (the corpus cases that reach the
mutated file). Paper 2 reports the two earlier liveness-ladder runs (66.5% and 62.9%) and states
that no reduction-side kill rate exists.
