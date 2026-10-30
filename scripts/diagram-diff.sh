#!/usr/bin/env bash
# Pin the diagram emitters: run two rdra-ish binaries over the same inputs and compare every
# byte they print or write. Use it to prove a refactor of crates/rdra-ish-emit changes no output.
# Usage: scripts/diagram-diff.sh BASE_BIN HEAD_BIN
#   build BASE_BIN from the commit before the refactor and HEAD_BIN (target/release/rdra-ish)
#   from the tree under test. Exits 1 when any cell differs.
#
# A cell is one `rdra-ish diagram` run. Per cell the dump holds the exit code, stdout and
# stderr, and the written .mmd. Errors are cells too, so their text is pinned.
# Axes:
#   roots    every samples.txt root plus every samples/incremental-order step
#   kind     every `diagram --kind` in KINDS. Set KINDS to a space-separated subset to compare only those kinds, e.g. while a unit of work
#            removes the others: KINDS="rdra er" scripts/diagram-diff.sh BASE HEAD
#   format   mermaid only, passed explicitly
#   scope    no scope, each --buc, the first two --buc together, and for usecase the first two
#            --usecase alone and together
#   options  --node-kind and --edge-kind on the kinds that take them
#   errors   every option the CLI rejects, per root, and the removed --format, --kind, and
#            option values once
# Set STRIP_COMMENTS=1 to drop the `%%` lines of each .mmd before comparing, to prove a change
# touched only a diagram's header and comments.
set -uo pipefail

base_bin="${1:?usage: scripts/diagram-diff.sh BASE_BIN HEAD_BIN}"
head_bin="${2:?usage: scripts/diagram-diff.sh BASE_BIN HEAD_BIN}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
base_bin="$(cd "$(dirname "$base_bin")" && pwd)/$(basename "$base_bin")"
head_bin="$(cd "$(dirname "$head_bin")" && pwd)/$(basename "$head_bin")"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

strip=()
[[ -n "${STRIP_COMMENTS:-}" ]] && strip=(-e '/^%%/d')
read -r -a kinds <<<"${KINDS:-rdra business usecase er state event}"
has_kind() { [[ " ${kinds[*]} " == *" $1 "* ]]; }
only_kinds() { # kind...: those of the arguments that KINDS keeps
  local kind
  for kind in "$@"; do has_kind "$kind" && echo "$kind"; done
}
graph_kinds=($(only_kinds rdra))
steps=(samples/incremental-order/step-*/src)

roots=()
while IFS= read -r root; do
  [[ -z "$root" || "$root" == \#* ]] && continue
  roots+=("$root")
done <samples.txt
for step in "${steps[@]}"; do
  [[ " ${roots[*]} " == *" $step "* ]] || roots+=("$step")
done

ids() { # root kind: first CSV column of `list`, header dropped
  "$head_bin" list "$1" --kind "$2" --format csv 2>/dev/null | tail -n +2 | cut -d, -f1
}

dump() { # label bin: one file per cell under $work/LABEL, numbered in run order
  local label="$1" bin="$2"
  local outdir="$work/$label.out" cells_dir="$work/$label" n=0
  mkdir -p "$outdir" "$cells_dir"

  cell() { # name args...
    local name="$1"
    shift
    rm -f "$outdir"/out.*
    local text rc out_file
    n=$((n + 1))
    out_file="$cells_dir/$(printf '%06d' "$n")"
    text="$("$bin" diagram "$@" --out "$outdir/out" 2>&1)"
    rc=$?
    {
      echo "### $name"
      echo "exit=$rc"
      printf '%s\n' "$text"
      local f
      for f in "$outdir/out.mmd"; do
        [[ -f "$f" ]] && { echo "--- ${f##*/}"; cat "$f"; }
      done
    } | sed -e "s|$repo_root/||g" -e "s|$outdir|OUT|g" ${strip[@]+"${strip[@]}"} >"$out_file"
  }

  local root kind buc fmt=mermaid
  for root in "${roots[@]}"; do
    local bucs=()
    while IFS= read -r line; do [[ -n "$line" ]] && bucs+=("$line"); done < <(ids "$root" buc)

    for kind in "${kinds[@]}"; do
      cell "$root $kind $fmt" "$root" --kind "$kind" --format "$fmt"
      for buc in ${bucs[@]+"${bucs[@]}"}; do
        cell "$root $kind $fmt buc=$buc" "$root" --kind "$kind" --format "$fmt" --buc "$buc"
      done
      if [[ ${#bucs[@]} -ge 2 ]]; then
        cell "$root $kind $fmt buc=${bucs[0]},${bucs[1]}" "$root" --kind "$kind" --format "$fmt" \
          --buc "${bucs[0]}" --buc "${bucs[1]}"
      fi
    done
    if has_kind usecase; then
      local ucs=()
      while IFS= read -r line; do [[ -n "$line" ]] && ucs+=("$line"); done < <(ids "$root" usecase)
      if [[ ${#ucs[@]} -ge 2 ]]; then
        cell "$root usecase $fmt usecase=${ucs[0]}" "$root" --kind usecase --format "$fmt" \
          --usecase "${ucs[0]}"
        cell "$root usecase $fmt usecase=${ucs[0]},${ucs[1]}" "$root" --kind usecase \
          --format "$fmt" --usecase "${ucs[0]}" --usecase "${ucs[1]}"
      fi
    fi
    for kind in ${graph_kinds[@]+"${graph_kinds[@]}"}; do
      cell "$root $kind $fmt node-kind" "$root" --kind "$kind" --format "$fmt" \
        --node-kind usecase --node-kind entity
      cell "$root $kind $fmt edge-kind" "$root" --kind "$kind" --format "$fmt" --edge-kind performs
      cell "$root $kind $fmt node+edge-kind" "$root" --kind "$kind" --format "$fmt" \
        --node-kind actor --node-kind usecase --edge-kind performs --edge-kind invokes
      if [[ ${#bucs[@]} -ge 1 ]]; then
        cell "$root $kind $fmt buc=${bucs[0]} node-kind" "$root" --kind "$kind" --format "$fmt" \
          --buc "${bucs[0]}" --node-kind usecase --node-kind entity --node-kind state
      fi
    done

    cell "$root error unknown-buc" "$root" --buc NoSuchBuc
    cell "$root error unknown-node-kind" "$root" --node-kind bogus --edge-kind bogus
    cell "$root error filter-on-er" "$root" --kind er --node-kind usecase
  done

  local removed
  for removed in puml svg png; do
    cell "samples/ec-site error format=$removed" samples/ec-site --format "$removed"
  done
  for removed in boundaryless-graph business-area technical-area sequence diff event-flow; do
    cell "samples/ec-site error kind=$removed" samples/ec-site --kind "$removed"
  done
  cell "samples/ec-site error option=--usecase" samples/ec-site --usecase Capture
  cell "samples/ec-site error unknown-usecase" samples/ec-site --kind usecase --usecase NoSuchUc
  cell "samples/ec-site error usecase+buc" samples/ec-site --kind usecase --usecase Capture \
    --buc BucPayment
  cell "samples/ec-site error option=--diff-base" samples/ec-site --diff-base samples/ec-site
  cell "samples/ec-site error option=--show-description" samples/ec-site --show-description
  cell "samples/ec-site error option=--view-preset" samples/ec-site --view-preset api
}

dump base "$base_bin" &
dump head "$head_bin" &
wait

cells="$(find "$work/head" -type f | wc -l | tr -d ' ')"
differ="$(diff -rq "$work/base" "$work/head" | wc -l | tr -d ' ')"
if [[ "$differ" -ne 0 ]]; then
  diff -r "$work/base" "$work/head" | head -200
  echo "differing cells:"
  diff -rq "$work/base" "$work/head" | awk '{print $4}' | while IFS= read -r f; do head -1 "$f"; done
fi
ok_cells="$(grep -lx "exit=0" "$work"/head/* | wc -l | tr -d " ")"
echo "diagram-diff: $cells cells ($ok_cells exit 0, the rest pin error text)" >&2
echo "diagram-diff: $cells cells, $differ differ"
[[ "$differ" -eq 0 ]]
