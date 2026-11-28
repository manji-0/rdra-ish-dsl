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

The `Mutants` workflow (`.github/workflows/mutants.yml`) with `config=tlc` mutates the TLA+ printer
(`crates/rdra-ish-emit/src/tla/`), the comparison (`verify/compare.rs`) and the TLC output reader
(`tlc_output.rs`, `tlc_replay.rs`). It tests each mutant with the engine corpus under TLC and z3.
No run of it had completed before this work: the one earlier run (46052978676 on branch
`mutants-tlc`, 2026-11-27) was cancelled. This work started run
[46128666288](https://github.com/manji-0/rdra-ish-dsl/actions/runs/46128666288) on main, with four
shards. MUTANTS-RESULT
