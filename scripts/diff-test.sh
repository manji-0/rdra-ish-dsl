#!/usr/bin/env bash
# Run the tests of the crates the given files belong to, with the tests CI's `test` step
# leaves to its release jobs skipped. The pre-commit hook passes the staged files.
#
#   scripts/diff-test.sh FILE...   the crates that own FILE...; nothing when none does
#   scripts/diff-test.sh --all     every crate (CI's `test` step)
#
# A change to the root Cargo.toml or Cargo.lock runs every crate. A crate's dependents are
# not run: CI runs the whole workspace on every push.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Run in release by engine-equivalence, formal-verify, and engine-corpus. `--skip` matches
# a part of the name, so a new `bfs_and_tlc_*` or `tlc_alone_*` test is skipped too.
skips=(
  --skip bfs_and_tlc_
  --skip tlc_alone_
  --skip a_property_cone_decides_what_the_whole_model_decides
  --skip reduced_domain_cones_decide_what_the_cones_decide
  --skip no_proved_rule_has_a_bounded_counterexample
)

packages=()
all=false
for f in "$@"; do
  case "$f" in
    --all | Cargo.toml | Cargo.lock) all=true ;;
    crates/*/*)
      crate=${f#crates/}
      crate=${crate%%/*}
      [[ " ${packages[*]-} " == *" -p $crate "* ]] || packages+=(-p "$crate")
      ;;
  esac
done

if $all; then
  exec cargo test --all -- "${skips[@]}"
elif ((${#packages[@]})); then
  exec cargo test "${packages[@]}" -- "${skips[@]}"
fi
echo "diff-test: no crate changed"
