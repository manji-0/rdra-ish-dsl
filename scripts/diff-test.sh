#!/usr/bin/env bash
# Run the tests of the crates the given files belong to under cargo-nextest, with the tests
# CI's `test` step leaves to its release jobs skipped. The pre-commit hook passes the staged
# files.
#
#   scripts/diff-test.sh FILE...   the crates that own FILE...; nothing when none does
#   scripts/diff-test.sh --all     every crate (CI's `test` step)
#
#   --profile NAME   a profile of .config/nextest.toml; CI's check job runs `pr`, which skips
#                    the slow tests it lists
#   --filter EXPR    the tests the nextest filterset EXPR matches in place of the profile's
#                    default filter, which EXPR can name as `default()`
#
# A change to the root Cargo.toml or Cargo.lock runs every crate. A crate's dependents are
# not run: CI's test step runs the whole workspace. The doctests run after, through
# `cargo test --doc`, which nextest does not run.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Run in release by engine-equivalence, formal-verify, and engine-corpus; proof_equivalence
# runs whole in engine-equivalence (in debug its solver-count test alone took 4 minutes).
# `test()` matches a part of the name, so a new `bfs_and_tlc_*` or `tlc_alone_*` test is
# skipped too.
release_only=(
  bfs_and_tlc_
  tlc_alone_
  a_property_cone_decides_what_the_whole_model_decides
  reduced_domain_cones_decide_what_the_cones_decide
  the_group_rules_of_every_cone_are_the_snapshot
  no_proved_rule_has_a_bounded_counterexample
  a_proof_is_the_same_on_any_number_of_solvers
)
filter=$(printf ' | test(%s)' "${release_only[@]}")
filter="not (${filter# | })"

packages=()
nextest=()
all=false
while (($#)); do
  case "$1" in
    --profile)
      nextest+=(--profile "$2")
      shift
      ;;
    --filter)
      # nextest otherwise intersects every filterset with the default filter.
      nextest+=(--ignore-default-filter)
      filter="$filter and ($2)"
      shift
      ;;
    --all | Cargo.toml | Cargo.lock) all=true ;;
    crates/*/*)
      crate=${1#crates/}
      crate=${crate%%/*}
      [[ " ${packages[*]-} " == *" -p $crate "* ]] || packages+=(-p "$crate")
      ;;
  esac
  shift
done

if $all; then
  packages=(--workspace)
elif ! ((${#packages[@]})); then
  echo "diff-test: no crate changed"
  exit 0
fi
# --no-tests=pass: a crate whose tests the filters all skip.
cargo nextest run "${packages[@]}" "${nextest[@]}" --no-tests=pass -E "$filter"
cargo test --doc "${packages[@]}"
