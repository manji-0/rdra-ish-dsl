#!/usr/bin/env bash
# Snapshot the data-layer exports (DBML, JSON Schema, entity CSV) for every sample in
# samples.txt so two runs can be compared with `diff -r target/data-diff/A target/data-diff/B`.
# Usage: scripts/data-diff.sh LABEL   (uses target/release/rdra-ish; build it first)
set -uo pipefail

label="${1:?usage: scripts/data-diff.sh LABEL}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

bin="${RDRA_BIN:-target/release/rdra-ish}"
root="target/data-diff/$label"
rm -rf "$root"

# Keep the label and checkout out of logs so two runs diff cleanly.
normalize() {
  sed -i.bak -e "s|$repo_root/||g" -e "s|$root|ROOT|g" "$1" && rm -f "$1.bak"
}

while IFS= read -r sample; do
  [[ -z "$sample" || "$sample" == \#* ]] && continue
  slug="${sample//\//__}"
  dir="$root/$slug"
  mkdir -p "$dir"

  "$bin" export "$sample" --kind dbml -o "$dir/schema.dbml" >"$dir/dbml.log" 2>&1
  echo $? >"$dir/dbml.exit"
  normalize "$dir/dbml.log"

  "$bin" export "$sample" --kind json-schema -o "$dir/json-schema.json" >"$dir/json-schema.log" 2>&1
  echo $? >"$dir/json-schema.exit"
  normalize "$dir/json-schema.log"

  "$bin" csv "$sample" --kind entity --out "$dir/entity.csv" >"$dir/entity.log" 2>&1
  echo $? >"$dir/entity.exit"
  normalize "$dir/entity.log"
done <samples.txt

echo "wrote $root"
