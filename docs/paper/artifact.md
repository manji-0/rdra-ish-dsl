# Artifact: regenerating every table (roadmap T19)

This page says how to rebuild the tool and regenerate each table of Paper 1 and Paper 2 from a
clean checkout. The toolchain is pinned by the repository's Nix flake (`flake.nix`, `flake.lock`):
Rust, TLC 1.8.0, z3 and elan (Lean 4.36.0, pinned in `proofs/lean/lean-toolchain`).

## Setup

```sh
nix develop                     # or: a Rust toolchain from rust-toolchain, python3, and for TLC a JDK
cargo build --release -p rdra-ish-cli --features perf --target-dir target/perf
BIN=target/perf/release/rdra-ish
```

The `perf` feature adds the `perf-run` and `perf-pass` lines to stderr that the scripts read
(states, edges and CPU per BFS run). It does not change any verdict.

## Tables

| Table | Command | Data | Time on the paper's machine |
|---|---|---|---|
| Ablation (Paper 1, 7.1) | `scripts/paper/eval.py ablation $BIN docs/paper/eval/data/ablation.tsv` | `eval/data/ablation.tsv` | about 45 min, mostly runs that hit the 300 s timeout (`RDRA_EVAL_TIMEOUT=300`) |
| Scaling (7.2) | `scripts/paper/eval.py scaling $BIN docs/paper/eval/data/scaling.tsv samples/patent-ops samples/clinic-ops` | `eval/data/scaling.tsv` | see [eval/scaling.md](./eval/scaling.md) |
| Least instance count (7.3) | `scripts/paper/eval.py minimal $BIN docs/paper/eval/data/minimal.tsv` | `eval/data/minimal.tsv` | about 1 min |
| Determinism (7.4) | `scripts/paper/eval.py determinism $BIN docs/paper/eval/data/determinism.tsv samples/patent-ops samples/clinic-ops` | `eval/data/determinism.tsv` | about 2 min |
| Whole model, no cone | `cargo run --release -p rdra-ish-core --example paper_whole -- samples/patent-ops 2 20000000` | printed | seconds to the state cap |
| TLC agreement (7.5) | `python3 -I scripts/paper/tlc-baseline.py --check docs/paper/eval/tlc-baseline.md` (from saved CI lines); a local rerun is `scripts/verify-samples.sh --with-tlc --tlc-roots scripts/tlc-roots.txt $BIN` | `eval/data/tlc-ci-lines.txt` | the local rerun is about 4 min on CI |
| Technical report | `python3 -I scripts/paper/build-tech-report.py` (and `--check`) | `tech-report-full.md` | instant |
| T15 cross-check of `proved` | `prove_transcript OUT ROOT` then `scripts/paper/smt-crosscheck.py OUT/<root>.smt2` | `eval/data/smt-crosscheck.txt` | minutes per root |
| T11 mutation sample | `cargo mutants --config .cargo/mutants-tlc.toml --baseline skip --timeout 900 --shard 0/12 --sharding round-robin` (tlc and z3 on PATH) | `eval/data/mutants-tlc-sample.txt` | about 3.5 h |
| T16 prover | `scripts/paper/prove-eval.py run $BIN docs/paper/eval/data/prove.tsv` | `eval/data/prove.tsv` | about 4 min |
| T8 local TLC | `scripts/paper/tlc-local.py run $BIN docs/paper/eval/data/tlc-local.tsv` (tlc on PATH) | `eval/data/tlc-local.tsv` | about 5 min |
| Defects (Paper 2) | `python3 -I scripts/paper/defect-summary.py` | `evidence/defects.tsv` | instant |
| Lean | `scripts/check-lean-proofs.sh` | `proofs/lean/` | about a minute from a warm elan cache |
| Lean checkers on the samples | `scripts/check-lean-certificates.sh`, `scripts/check-lean-partitions.sh` | dumped splits and partitions | minutes |

Every Markdown table under `eval/` is the output of `scripts/paper/eval.py table KIND FILE.tsv` on
its data file. Rerunning a command overwrites the TSV. Compare the verdict columns, not the times:
the times depend on the machine and its load.

## The paper's machine

A 4-core x86_64 Linux VM with 15 GiB of memory, kernel 6.18, with no other load during each run
(`uptime` below 1.5 at the start). All runs used `RDRA_JOBS=1` except the determinism table, which
varies it.
