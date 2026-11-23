#!/usr/bin/env bash
# Run `check` and `verify` on every sample root in samples.txt and compare each run with the
# `// expect` headers in the root's first .rdra file in load order (`rdra-ish files`).
# The header grammar is in docs/formal-verification.md ("Sample expectations").
# Usage: scripts/verify-samples.sh [--with-tlc] <rdra-ish binary>
#
# Without --with-tlc every run has no `tlc` or `tlc2` on PATH and no RDRA_TLC_JAR, so a
# passing run proves that `verify` needs no JVM. With --with-tlc every `verify` run also
# runs TLC beside the native checker: it must match the same header, print `tlc_result:`, and
# report no engine disagreement, which would add an error name the header does not list.
set -uo pipefail

with_tlc=false
if [[ "${1:-}" == --with-tlc ]]; then
  with_tlc=true
  shift
fi
bin="${1:?usage: scripts/verify-samples.sh [--with-tlc] <rdra-ish binary>}"
[[ "$bin" == /* ]] || bin="$PWD/$bin"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root" || exit 2

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

timer="$(type -P time || true)"
if [[ -z "$timer" ]]; then
  echo "verify-samples: no time binary on PATH (GNU time or BSD /usr/bin/time)" >&2
  exit 2
fi
gnu_time=false
"$timer" -v true >/dev/null 2>&1 && gnu_time=true

# PATH without the directories that hold `tlc` or `tlc2`, for the runs without --with-tlc.
run_path="$PATH"
if ! $with_tlc; then
  run_path="$(tr ':' '\n' <<<"$PATH" | while IFS= read -r d; do
    [[ -n "$d" && ! -e "$d/tlc" && ! -e "$d/tlc2" ]] && printf '%s\n' "$d"
  done | paste -sd: -)"
  if PATH="$run_path" type -P tlc tlc2 >/dev/null; then
    echo "verify-samples: tlc is still on PATH after removing it" >&2
    exit 2
  fi
fi

name_re='[A-Za-z0-9_]+'
check_re="^(ok|fail( $name_re)+)$"
# `tlc=` names TLC's verdict, so only a line whose flags hold --with-tlc may carry it.
tlc_re="^(tlc=(ok|counterexample)( fail( $name_re)+)?|ok|fail( $name_re)+)$"
header_re='^// expect (check|verify)( [^:]*)?: (.*)$'

# Runs "$@" with combined output in $work/out; sets status, wall (s), and rss (MB).
measure() {
  if $gnu_time; then
    "$timer" -f '%e %M' -o "$work/time" "$@" >"$work/out" 2>&1
    status=$?
    read -r wall rss < <(tail -n 1 "$work/time")
    rss=$((rss / 1024))
  else
    "$timer" -l -o "$work/time" "$@" >"$work/out" 2>&1
    status=$?
    wall="$(awk '/ real /{print $1}' "$work/time")"
    rss="$(awk '/maximum resident set size/{print int($1 / 1048576)}' "$work/time")"
  fi
}

# The names of the run's errors: each finding's `error[<name>]:` and TLC's own check names.
error_names() {
  sed -nE \
    -e 's/^error\[([A-Za-z0-9_]+)\]:.*/\1/p' \
    -e 's/^violated: Error: (Invariant|Action property) ([^ ]+) is violated\..*/\2/p' \
    "$1" | sort -u | tr '\n' ' ' | sed 's/ $//'
}

roots=0
runs=0
mismatches=0

while IFS= read -r root; do
  [[ -z "$root" || "$root" == \#* ]] && continue
  roots=$((roots + 1))

  if ! "$bin" files "$root" >"$work/files" 2>&1; then
    echo "MISMATCH $root: rdra-ish files failed: $(cat "$work/files")"
    mismatches=$((mismatches + 1))
    continue
  fi
  first="$(head -n 1 "$work/files")"
  stray="$(tail -n +2 "$work/files" | while IFS= read -r f; do
    grep -l '^// expect' "$f"
  done)"
  if [[ -n "$stray" ]]; then
    echo "MISMATCH $root: // expect headers outside the first file $first: $stray"
    mismatches=$((mismatches + 1))
    continue
  fi

  keys=()
  exps=()
  malformed=""
  while IFS= read -r line; do
    if [[ ! "$line" =~ $header_re ]]; then
      malformed="$line"
      break
    fi
    cmd="${BASH_REMATCH[1]}"
    flags="${BASH_REMATCH[2]# }"
    exp="${BASH_REMATCH[3]}"
    re="$check_re"
    [[ "$cmd" == verify && " $flags " == *" --with-tlc "* ]] && re="$tlc_re"
    if [[ ! "$exp" =~ $re ]]; then
      malformed="$line"
      break
    fi
    for k in ${keys[@]+"${keys[@]}"}; do
      [[ "$k" == "$cmd|$flags" ]] && malformed="$line (duplicate)"
    done
    [[ -n "$malformed" ]] && break
    keys+=("$cmd|$flags")
    exps+=("$exp")
  done < <(grep '^// expect' "$first")
  if [[ -n "$malformed" ]]; then
    echo "MISMATCH $root: malformed header in $first: $malformed"
    mismatches=$((mismatches + 1))
    continue
  fi
  # A command with no header line runs once without flags. A root whose lines for a
  # command are all flagged runs only those: the flags state the bounds it is checked at.
  for default in "check|ok" "verify|ok"; do
    found=false
    for k in ${keys[@]+"${keys[@]}"}; do
      [[ "${k%%|*}" == "${default%%|*}" ]] && found=true
    done
    if ! $found; then
      keys+=("${default%%|*}|")
      exps+=("${default#*|}")
    fi
  done

  for i in "${!keys[@]}"; do
    cmd="${keys[$i]%%|*}"
    flags="${keys[$i]#*|}"
    exp="${exps[$i]}"
    # A line that asks for TLC runs only in the --with-tlc mode, which has TLC on PATH.
    if ! $with_tlc && [[ " $flags " == *" --with-tlc "* ]]; then
      printf '%-8s %-56s %s\n' skip "$root" "$cmd $flags"
      continue
    fi
    runs=$((runs + 1))

    want_exit=0
    want_tlc=none
    want_names=""
    for tok in $exp; do
      case "$tok" in
        ok) ;;
        fail) want_exit=1 ;;
        tlc=*) want_tlc="${tok#tlc=}" ;;
        *) want_names="$want_names $tok" ;;
      esac
    done
    want_names="$(tr ' ' '\n' <<<"$want_names" | sed '/^$/d' | sort -u | tr '\n' ' ' | sed 's/ $//')"

    rm -rf "$work/tla"
    extra=()
    if $with_tlc && [[ "$cmd" == verify && " $flags " != *" --with-tlc "* ]]; then
      extra=(--with-tlc)
    fi
    # shellcheck disable=SC2086 # header flags are CLI words
    if [[ "$cmd" == check ]]; then
      measure env -u RDRA_TLC_JAR PATH="$run_path" "$bin" check "$root" $flags
    else
      measure env -u RDRA_TLC_JAR PATH="$run_path" "$bin" verify "$root" -o "$work/tla" ${extra[@]+"${extra[@]}"} $flags
    fi
    got_tlc="$(sed -nE 's/^tlc_result: ([a-z_]+).*/\1/p' "$work/out" | head -n 1)"
    got_tlc="${got_tlc:-none}"
    got_names="$(error_names "$work/out")"
    # Without a `tlc=` the header says nothing of TLC's verdict, but a run with TLC prints one.
    if [[ "$want_tlc" == none ]]; then
      want_tlc="$got_tlc"
      # A run that states its result under --with-tlc states TLC's beside it.
      if [[ ${#extra[@]} -gt 0 && "$got_tlc" == none ]] && grep -q '^result: ' "$work/out"; then
        want_tlc=printed
      fi
    fi
    # A TLC counterexample must carry TLC's own violation line, not only our rendering.
    tlc_line=true
    if [[ "$want_tlc" == counterexample ]] && ! grep -q '^violated: Error: .* violated' "$work/out"; then
      tlc_line=false
    fi
    # An error without a name cannot be matched against the header.
    unnamed="$(grep -c '^error: ' "$work/out")"

    label="$cmd${flags:+ $flags}"
    if [[ "$status" == "$want_exit" && "$got_tlc" == "$want_tlc" && "$got_names" == "$want_names" && "$unnamed" == 0 ]] && $tlc_line; then
      verdict=ok
    else
      verdict=MISMATCH
      mismatches=$((mismatches + 1))
    fi
    printf '%-8s %-56s %-22s %7ss %6s MB\n' "$verdict" "$root" "$label" "$wall" "$rss"
    if [[ "$verdict" == MISMATCH ]]; then
      echo "  expected: exit=$want_exit tlc_result=$want_tlc errors=[$want_names]  ($exp)"
      echo "  actual:   exit=$status tlc_result=$got_tlc errors=[$got_names]"
      $tlc_line || echo "  no TLC 'violated: Error: ...' line for the counterexample"
      [[ "$unnamed" == 0 ]] || echo "  $unnamed unnamed 'error: ...' line(s); every error must print error[<name>]:"
      tail -n 40 "$work/out" | sed 's/^/  | /'
    fi
  done
done <samples.txt

echo "verify-samples: $roots roots, $runs runs, $mismatches mismatches"
[[ "$roots" -gt 0 && "$mismatches" -eq 0 ]]
