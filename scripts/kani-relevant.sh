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
    crates/rdra-ish-core/src/lifecycle/explore/packing.rs) relevant=true ;;
    crates/rdra-ish-core/src/lifecycle/explore/segments.rs) relevant=true ;;
    crates/rdra-ish-core/src/lifecycle/explore/store.rs) relevant=true ;;
    crates/rdra-ish-core/src/lifecycle/explore/table.rs) relevant=true ;;
    crates/rdra-ish-core/src/lifecycle/explore/symmetry/mod.rs) relevant=true ;;
    # `one_of`'s harness, its evaluator of `smt.rs` terms, and the choices of `build/sets.rs`
    # (`guarded_set`) that it models.
    crates/rdra-ish-core/src/lifecycle/prove/encode.rs) relevant=true ;;
    crates/rdra-ish-core/src/lifecycle/prove/smt.rs) relevant=true ;;
    crates/rdra-ish-core/src/lifecycle/build/sets.rs) relevant=true ;;
    # The canonical form: `symmetry/mod.rs` builds it into the harness's module, though the
    # harness proves `Group::apply` and `Group::compose` and calls no function of it.
    crates/rdra-ish-core/src/lifecycle/explore/symmetry/canon.rs) relevant=true ;;
    # The domain harnesses, and what the others run: `CmpOp`, `Domain`, `Value` and `EntId`
    # (types.rs), `Edge` (explore/mod.rs).
    crates/rdra-ish-core/src/lifecycle/types.rs) relevant=true ;;
    crates/rdra-ish-core/src/lifecycle/explore/mod.rs) relevant=true ;;
    # The build's `DomainTooLarge` gate: no harness runs it, but it calls the
    # `Domain::column_values` and `Domain::admits` the domain harnesses prove, and a change
    # to that call changes what their proof is evidence for.
    crates/rdra-ish-core/src/lifecycle/build/vars.rs) relevant=true ;;
    # The build: dependencies, features, the `cfg(kani)` lint, and the Kani pin.
    Cargo.toml | Cargo.lock | crates/rdra-ish-core/Cargo.toml) relevant=true ;;
    .github/workflows/ci.yml | scripts/kani-relevant.sh) relevant=true ;;
  esac
done

echo "$relevant"
