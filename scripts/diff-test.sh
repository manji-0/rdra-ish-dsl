#!/usr/bin/env bash
# Run the tests of the crates the given files belong to, with the tests CI's `test` step
# leaves to its release jobs skipped. The pre-commit hook passes the staged files.
#
#   scripts/diff-test.sh FILE...   the crates that own FILE...; nothing when none does
#   scripts/diff-test.sh --all     every crate (CI's `test` step)
#
#   --profile NAME   a profile of .config/nextest.toml; `pr` by default, which skips the slow
#                    tests it lists, as CI's test step does on a pull request
#   --filter EXPR    the tests the nextest filterset EXPR matches in place of the profile's
#                    default filter, which EXPR can name as `default()`
#   --nextest-only   only the cargo-nextest run: not the diagram_contract binary nor the
#                    doctests, which the run without this flag has run (CI's `slow tests`)
#
# The tests run under cargo-nextest, but for two that run through `cargo test` after it:
# the doctests, which nextest does not run, and rdra-ish-cli's diagram_contract binary, whose
# tests share one load and exploration of the samples in a process and take about 35 s
# together; nextest's process per test made nine of them take over 10 s each.
#
# A change to the root Cargo.toml or Cargo.lock runs every crate. A crate's dependents are
# not run: CI's test step runs the whole workspace.
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
  reduced_domain_cones_hold_each_fold_bit_where_its_action_runs
  reduced_domain_cones_step_by_a_true_fold_bit_only_where_the_cone_steps
  case_fold_bits_hold_exactly_where_their_actions_run_past_slot_0
  reduced_cones_step_as_their_cones_step
  the_group_rules_of_every_cone_are_the_snapshot
  no_proved_rule_has_a_bounded_counterexample
  a_proof_is_the_same_on_any_number_of_solvers
)
filter=$(printf ' | test(%s)' "${release_only[@]}")
filter="not (${filter# | }) and not binary_id(rdra-ish-cli::diagram_contract)"

packages=()
nextest=()
profile=pr
all=false
nextest_only=false
while (($#)); do
  case "$1" in
    --profile)
      profile=$2
      shift
      ;;
    --filter)
      # nextest otherwise intersects every filterset with the default filter.
      nextest+=(--ignore-default-filter)
      filter="$filter and ($2)"
      shift
      ;;
    --nextest-only) nextest_only=true ;;
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
cargo nextest run "${packages[@]}" --profile "$profile" ${nextest[@]+"${nextest[@]}"} --no-tests=pass -E "$filter"
$nextest_only && exit 0
if $all || [[ " ${packages[*]} " == *" -p rdra-ish-cli "* ]]; then
  # With the packages nextest built, so cargo resolves the same features and builds nothing.
  cargo test "${packages[@]}" --test diagram_contract
fi
cargo test --doc "${packages[@]}"
