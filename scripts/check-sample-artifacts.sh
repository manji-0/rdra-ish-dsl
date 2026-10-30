#!/usr/bin/env bash
# Regenerate every committed sample artifact listed in scripts/sample-artifacts.tsv, and the
# sample documents that embed them, and fail when the result differs from what is committed.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

manifest="scripts/sample-artifacts.tsv"

cargo build --quiet --bin rdra-ish
bin="target/debug/rdra-ish"

outputs=()
while IFS=$'\t' read -r out cmd; do
  [[ -z "$out" || "$out" == \#* ]] && continue
  outputs+=("$out")
  mkdir -p "$(dirname "$out")"
  read -r -a args <<<"$cmd"
  case "${args[0]}" in
    # `states` prints to stdout.
    states) "$bin" "${args[@]}" >"$out" ;;
    # `diagram` appends the extension of its format to --out.
    diagram) "$bin" "${args[@]}" --out "${out%.*}" ;;
    *) "$bin" "${args[@]}" --out "$out" ;;
  esac
done <"$manifest"

# The documents that embed the artifacts above.
python3 -I scripts/embed-sample-artifacts.py
python3 -I scripts/gen-clinic-ops-design-sample.py
outputs+=(samples/incremental-order/step-*/design.md samples/clinic-ops/design-sample.md)

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
