#!/usr/bin/env bash
# Snapshot engine outputs for every sample in samples.txt so two runs can be
# compared with `diff -r target/engine-diff/A target/engine-diff/B`.
# Usage: scripts/engine-diff.sh LABEL   (uses target/release/rdra-ish; build it first)
set -uo pipefail

label="${1:?usage: scripts/engine-diff.sh LABEL}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

bin="${RDRA_BIN:-target/release/rdra-ish}"
root="target/engine-diff/$label"
rm -rf "$root"

have_tlc=false
command -v tlc >/dev/null 2>&1 && have_tlc=true

while IFS= read -r sample; do
  [[ -z "$sample" || "$sample" == \#* ]] && continue
  slug="${sample//\//__}"
  dir="$root/$slug"
  mkdir -p "$dir/tla"

  "$bin" states "$sample" --format json >"$dir/states.json" 2>&1
  echo $? >"$dir/states.exit"

  "$bin" check "$sample" >"$dir/check.txt" 2>&1
  echo "exit=$?" >>"$dir/check.txt"
  sed -i.bak -e "s|$repo_root/||g" "$dir/check.txt" && rm -f "$dir/check.txt.bak"

  "$bin" export "$sample" --kind tla -o "$dir/tla" >"$dir/export.log" 2>&1
  echo $? >"$dir/export.exit"
  # Keep the label out of the log so two runs diff cleanly.
  sed -i.bak "s|$root|ROOT|g" "$dir/export.log" && rm -f "$dir/export.log.bak"
  # The exporter names its files RdraSpec.*; snapshot them as model.*.
  [[ -f "$dir/tla/RdraSpec.tla" ]] && cp "$dir/tla/RdraSpec.tla" "$dir/model.tla"
  [[ -f "$dir/tla/RdraSpec.cfg" ]] && cp "$dir/tla/RdraSpec.cfg" "$dir/model.cfg"
  true

  if $have_tlc; then
    timeout "${RDRA_VERIFY_TIMEOUT:-300}" "$bin" verify "$sample" --backend tlc -o "$dir/tla" >"$dir/verify.txt" 2>&1
    rc=$?
    [[ $rc -eq 124 ]] && echo "tlc_result: timeout after ${RDRA_VERIFY_TIMEOUT:-300}s" >>"$dir/verify.txt"
    echo "exit=$rc" >>"$dir/verify.txt"
    sed -i.bak -e "s|$repo_root/||g" -e "s|$root|ROOT|g" "$dir/verify.txt" && rm -f "$dir/verify.txt.bak"
    # Drop TLC run noise (seed, timestamps, its states/ work dir) so verdicts diff cleanly.
    grep -vE 'seed -?[0-9]+|^ *Parsing file |^ *Starting\.\.\. \(|Finished in .* at \(' "$dir/verify.txt" >"$dir/verify.txt.tmp"
    mv "$dir/verify.txt.tmp" "$dir/verify.txt"
    rm -rf "$dir/tla/states"
  else
    echo skipped >"$dir/verify.txt"
  fi
done <samples.txt

echo "wrote $root"
