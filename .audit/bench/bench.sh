#!/usr/bin/env bash
# Usage: bench.sh <rdra-ish binary> <label>. Median of 5 wall-clock runs per command.
set -uo pipefail
bin=$1; label=$2
out=$(mktemp -d)
cmds=(
  "check samples/clinic-ops"
  "list samples/clinic-ops --kind rule --format csv"
  "csv samples/clinic-ops --kind matrix"
  "states samples/clinic-ops"
  "diagram samples/clinic-ops --all --out $out/d"
  "export samples/insurance-claim-ops --kind sql --out $out/s.sql"
)
for c in "${cmds[@]}"; do
  ts=()
  for i in 1 2 3 4 5; do
    s=$(date +%s.%N); $bin $c >/dev/null 2>&1; rc=$?; e=$(date +%s.%N)
    ts+=("$(echo "$e - $s" | bc)")
  done
  med=$(printf '%s\n' "${ts[@]}" | sort -n | sed -n 3p)
  printf '%s\t%s\t%.3f\trc=%s\n' "$label" "${c%% --out*}" "$med" "$rc"
done
rm -rf "$out"
