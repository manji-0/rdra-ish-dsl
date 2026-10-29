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
#   kind     every `diagram --kind` in KINDS (diff has its own axis below). Set KINDS to a
#            space-separated subset to compare only those kinds, e.g. while a unit of work
#            removes the others: KINDS="rdra er" scripts/diagram-diff.sh BASE HEAD
#   format   mermaid only, passed explicitly
#   scope    no scope, each --buc, the first two --buc together, each --usecase
#            (sequence, the only kind that takes it)
#   options  --show-description; each --view-preset, --node-kind and --edge-kind on the
#            kinds that take them
#   diff     --kind diff across consecutive incremental-order steps and two unrelated roots
#   errors   every option the CLI rejects, per root, and the removed --format and --kind
#            values once
set -uo pipefail

base_bin="${1:?usage: scripts/diagram-diff.sh BASE_BIN HEAD_BIN}"
head_bin="${2:?usage: scripts/diagram-diff.sh BASE_BIN HEAD_BIN}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
base_bin="$(cd "$(dirname "$base_bin")" && pwd)/$(basename "$base_bin")"
head_bin="$(cd "$(dirname "$head_bin")" && pwd)/$(basename "$head_bin")"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

read -r -a kinds <<<"${KINDS:-rdra er state sequence event-flow diff}"
has_kind() { [[ " ${kinds[*]} " == *" $1 "* ]]; }
only_kinds() { # kind...: those of the arguments that KINDS keeps
  local kind
  for kind in "$@"; do has_kind "$kind" && echo "$kind"; done
}
graph_kinds=($(only_kinds rdra))
usecase_kinds=($(only_kinds sequence))
presets=(business system data api ui)
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
    } | sed -e "s|$repo_root/||g" -e "s|$outdir|OUT|g" >"$out_file"
  }

  local root kind buc uc preset fmt=mermaid
  for root in "${roots[@]}"; do
    local bucs=() ucs=()
    while IFS= read -r line; do [[ -n "$line" ]] && bucs+=("$line"); done < <(ids "$root" buc)
    while IFS= read -r line; do [[ -n "$line" ]] && ucs+=("$line"); done < <(ids "$root" usecase)

    for kind in "${kinds[@]}"; do
      [[ "$kind" == diff ]] && continue
      cell "$root $kind $fmt" "$root" --kind "$kind" --format "$fmt"
      cell "$root $kind $fmt show-description" "$root" --kind "$kind" --format "$fmt" --show-description
      for buc in ${bucs[@]+"${bucs[@]}"}; do
        cell "$root $kind $fmt buc=$buc" "$root" --kind "$kind" --format "$fmt" --buc "$buc"
      done
      if [[ ${#bucs[@]} -ge 2 ]]; then
        cell "$root $kind $fmt buc=${bucs[0]},${bucs[1]}" "$root" --kind "$kind" --format "$fmt" \
          --buc "${bucs[0]}" --buc "${bucs[1]}"
      fi
    done
    for kind in ${graph_kinds[@]+"${graph_kinds[@]}"}; do
      for preset in "${presets[@]}"; do
        cell "$root $kind $fmt preset=$preset" "$root" --kind "$kind" --format "$fmt" --view-preset "$preset"
      done
      cell "$root $kind $fmt node-kind" "$root" --kind "$kind" --format "$fmt" \
        --node-kind usecase --node-kind entity
      cell "$root $kind $fmt edge-kind" "$root" --kind "$kind" --format "$fmt" --edge-kind performs
      cell "$root $kind $fmt node+edge-kind" "$root" --kind "$kind" --format "$fmt" \
        --node-kind actor --node-kind usecase --edge-kind performs --edge-kind invokes
      if [[ ${#bucs[@]} -ge 1 ]]; then
        cell "$root $kind $fmt buc=${bucs[0]} preset=system" "$root" --kind "$kind" --format "$fmt" \
          --buc "${bucs[0]}" --view-preset system
      fi
    done
    for kind in ${usecase_kinds[@]+"${usecase_kinds[@]}"}; do
      for uc in ${ucs[@]+"${ucs[@]}"}; do
        cell "$root $kind $fmt usecase=$uc" "$root" --kind "$kind" --format "$fmt" --usecase "$uc"
      done
      if [[ ${#ucs[@]} -ge 2 ]]; then
        cell "$root $kind $fmt usecase=${ucs[0]},${ucs[1]}" "$root" --kind "$kind" --format "$fmt" \
          --usecase "${ucs[0]}" --usecase "${ucs[1]}"
      fi
    done

    cell "$root error unknown-buc" "$root" --buc NoSuchBuc
    cell "$root error unknown-usecase" "$root" --kind sequence --usecase NoSuchUc
    cell "$root error usecase-on-rdra" "$root" --kind rdra --usecase NoSuchUc
    cell "$root error unknown-node-kind" "$root" --node-kind bogus --edge-kind bogus
    cell "$root error filter-on-er" "$root" --kind er --node-kind usecase
    cell "$root error preset-on-state" "$root" --kind state --view-preset business
    cell "$root error diff-without-base" "$root" --kind diff
    cell "$root error diff-base-on-rdra" "$root" --diff-base samples/ec-site
    if [[ ${#bucs[@]} -ge 1 && ${#ucs[@]} -ge 1 ]]; then
      cell "$root error buc-with-usecase" "$root" --kind sequence --buc "${bucs[0]}" --usecase "${ucs[0]}"
    fi
  done

  local removed
  for removed in puml svg png; do
    cell "samples/ec-site error format=$removed" samples/ec-site --format "$removed"
  done
  for removed in boundaryless-graph business-area technical-area; do
    cell "samples/ec-site error kind=$removed" samples/ec-site --kind "$removed"
  done

  local pairs=() i
  for ((i = 1; i < ${#steps[@]}; i++)); do
    pairs+=("${steps[i]}|${steps[i - 1]}")
  done
  pairs+=("samples/ec-site|samples/personal-info" "samples/ec-site|samples/ec-site"
    "samples/clinic-ops|samples/ec-site")
  local pair head_root base_root fmt=mermaid
  has_kind diff || pairs=()
  for pair in ${pairs[@]+"${pairs[@]}"}; do
    head_root="${pair%%|*}"
    base_root="${pair##*|}"
    local bucs=()
    while IFS= read -r line; do [[ -n "$line" ]] && bucs+=("$line"); done < <(ids "$head_root" buc)
    cell "diff $head_root vs $base_root $fmt" "$head_root" --kind diff --format "$fmt" --diff-base "$base_root"
    cell "diff $head_root vs $base_root $fmt show-description" "$head_root" --kind diff --format "$fmt" \
      --diff-base "$base_root" --show-description
    for buc in ${bucs[@]+"${bucs[@]}"}; do
      cell "diff $head_root vs $base_root $fmt buc=$buc" "$head_root" --kind diff --format "$fmt" \
        --diff-base "$base_root" --buc "$buc"
    done
    for preset in "${presets[@]}"; do
      cell "diff $head_root vs $base_root $fmt preset=$preset" "$head_root" --kind diff --format "$fmt" \
        --diff-base "$base_root" --view-preset "$preset"
    done
    cell "diff $head_root vs $base_root $fmt node+edge-kind" "$head_root" --kind diff --format "$fmt" \
      --diff-base "$base_root" --node-kind usecase --node-kind entity --edge-kind performs --edge-kind reads
  done
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
