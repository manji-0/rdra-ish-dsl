#!/usr/bin/env bash
# Read changed paths on stdin, one per line; print `true` when one of them can change what
# the Kani harnesses prove, else `false`.
#
#   git diff --name-only HEAD^1 HEAD | scripts/kani-relevant.sh
#
# The list below names the files that hold a harness, the files whose code a harness runs,
# the crate manifests and the lockfile, and the workflow that pins the Kani version. A new
# harness in another file adds that file here. A file not listed cannot change what a
# harness proves, though it can break the core's build under Kani; the scheduled run on
# main catches that. An empty list prints `false`.
set -euo pipefail

relevant=false
while IFS= read -r path; do
  case "$path" in
    # The harnesses, next to the code they prove.
    crates/rdra-ish-core/src/lifecycle/expr.rs) relevant=true ;;
    crates/rdra-ish-core/src/lifecycle/explore/arena.rs) relevant=true ;;
    crates/rdra-ish-core/src/lifecycle/explore/edges.rs) relevant=true ;;
    crates/rdra-ish-core/src/lifecycle/explore/segments.rs) relevant=true ;;
    crates/rdra-ish-core/src/lifecycle/explore/store.rs) relevant=true ;;
    crates/rdra-ish-core/src/lifecycle/explore/symmetry/mod.rs) relevant=true ;;
    # The canonical form the symmetry harness runs.
    crates/rdra-ish-core/src/lifecycle/explore/symmetry/canon.rs) relevant=true ;;
    # What they run: `CmpOp`, `Domain`, `Value` and `EntId` (types.rs), `Edge` (explore/mod.rs).
    crates/rdra-ish-core/src/lifecycle/types.rs) relevant=true ;;
    crates/rdra-ish-core/src/lifecycle/explore/mod.rs) relevant=true ;;
    # The build: dependencies, features, the `cfg(kani)` lint, and the Kani pin.
    Cargo.toml | Cargo.lock | crates/rdra-ish-core/Cargo.toml) relevant=true ;;
    .github/workflows/ci.yml | scripts/kani-relevant.sh) relevant=true ;;
  esac
done

echo "$relevant"
