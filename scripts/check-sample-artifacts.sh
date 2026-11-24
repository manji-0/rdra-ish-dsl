#!/usr/bin/env bash
# Check that every sample is formatted, regenerate every committed sample artifact listed in
# scripts/sample-artifacts.tsv and the sample documents that embed them, and fail when the
# result differs from what is committed.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

manifest="scripts/sample-artifacts.tsv"

cargo build --quiet --bin rdra-ish
bin="target/debug/rdra-ish"

# Every sample and skill fixture is in the layout `fmt` writes.
"$bin" fmt --check samples skills

# The rows write separate files and read only the samples, so they run in parallel, one
# per core unless ARTIFACT_JOBS says otherwise. A row that fails makes xargs exit non-zero,
# after the other rows finish.
jobs="${ARTIFACT_JOBS:-$(getconf _NPROCESSORS_ONLN)}"

# One manifest row: the output path, then the command and its arguments.
write_row() {
  local out="$1"
  shift
  mkdir -p "$(dirname "$out")"
  case "$1" in
    # `states` and `list` print to stdout.
    states | list) "$bin" "$@" >"$out" ;;
    # `diagram` appends the extension of its format to --out.
    diagram) "$bin" "$@" --out "${out%.*}" ;;
    *) "$bin" "$@" --out "$out" ;;
  esac
}
export -f write_row
export bin

outputs=()
rows=()
while IFS=$'\t' read -r out cmd; do
  [[ -z "$out" || "$out" == \#* ]] && continue
  outputs+=("$out")
  rows+=("$out $cmd")
done <"$manifest"
# xargs splits each row on blanks, as `read -a` did; a row holds no quotes.
printf '%s\n' "${rows[@]}" | xargs -P "$jobs" -L 1 bash -c 'write_row "$@"' write_row

# The documents that embed the artifacts above, once every row is written. Each writes its
# own files, so they run side by side; each one's exit status is waited for.
pids=()
python3 -I scripts/embed-sample-artifacts.py &
pids+=($!)
for sample in clinic-ops patent-ops food-permit-ops insurance-claim-ops; do
  python3 -I scripts/gen-clinic-ops-docs.py "$bin" "$sample" &
  pids+=($!)
done
for pid in "${pids[@]}"; do
  wait "$pid"
done
outputs+=(samples/incremental-order/step-*/design.md samples/clinic-ops/design-doc.md samples/clinic-ops/prd.md
  samples/patent-ops/design-doc.md samples/patent-ops/prd.md
  samples/food-permit-ops/design-doc.md samples/food-permit-ops/prd.md
  samples/insurance-claim-ops/design-doc.md samples/insurance-claim-ops/prd.md
  docs/diagram-sample-review.md)

if ! git diff --quiet -- "${outputs[@]}"; then
  echo "sample artifacts changed; review and commit the generated diff if intentional" >&2
  git --no-pager diff -- "${outputs[@]}" >&2
  exit 1
fi

untracked="$(git ls-files --others --exclude-standard -- "${outputs[@]}")"
if [[ -n "$untracked" ]]; then
  if [[ "${CI:-false}" == "true" ]]; then
    echo "sample artifact generation produced untracked files:" >&2
    echo "$untracked" >&2
    exit 1
  fi
  echo "warning: sample artifact generation produced untracked files:" >&2
  echo "$untracked" >&2
fi

echo "sample artifacts are up to date"
