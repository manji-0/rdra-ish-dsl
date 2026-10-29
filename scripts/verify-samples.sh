#!/usr/bin/env bash
# Run `check` and `verify` on every sample root in samples.txt and compare each run with the
# `// expect` headers in the root's first .rdra file in load order (`rdra-ish files`).
# The header grammar is in docs/formal-verification.md ("Sample expectations").
# Usage: scripts/verify-samples.sh <rdra-ish binary>
set -uo pipefail

bin="${1:?usage: scripts/verify-samples.sh <rdra-ish binary>}"
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

name_re='[A-Za-z0-9_]+'
check_re="^(ok|fail( $name_re)+)$"
verify_re="^(tlc=(ok|counterexample)( fail( $name_re)+)?|fail( $name_re)+)$"
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
    if [[ "$cmd" == check ]]; then re="$check_re"; else re="$verify_re"; fi
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
  for default in "check|ok" "verify|tlc=ok"; do
    found=false
    for k in ${keys[@]+"${keys[@]}"}; do
      [[ "$k" == "${default%%|*}|" ]] && found=true
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
    # shellcheck disable=SC2086 # header flags are CLI words
    if [[ "$cmd" == check ]]; then
      measure "$bin" check "$root" $flags
    else
      measure "$bin" verify "$root" --backend tlc -o "$work/tla" $flags
    fi
    got_tlc="$(sed -nE 's/^tlc_result: ([a-z_]+).*/\1/p' "$work/out" | head -n 1)"
    got_tlc="${got_tlc:-none}"
    got_names="$(error_names "$work/out")"
    [[ "$cmd" == check ]] && want_tlc="$got_tlc"
    # A TLC counterexample must carry TLC's own violation line, not only our rendering.
    native=true
    if [[ "$want_tlc" == counterexample ]] && ! grep -q '^violated: Error: .* violated' "$work/out"; then
      native=false
    fi
    # An error without a name cannot be matched against the header.
    unnamed="$(grep -c '^error: ' "$work/out")"

    label="$cmd${flags:+ $flags}"
    if [[ "$status" == "$want_exit" && "$got_tlc" == "$want_tlc" && "$got_names" == "$want_names" && "$unnamed" == 0 ]] && $native; then
      verdict=ok
    else
      verdict=MISMATCH
      mismatches=$((mismatches + 1))
    fi
    printf '%-8s %-56s %-22s %7ss %6s MB\n' "$verdict" "$root" "$label" "$wall" "$rss"
    if [[ "$verdict" == MISMATCH ]]; then
      echo "  expected: exit=$want_exit tlc_result=$want_tlc errors=[$want_names]  ($exp)"
      echo "  actual:   exit=$status tlc_result=$got_tlc errors=[$got_names]"
      $native || echo "  no TLC 'violated: Error: ...' line for the counterexample"
      [[ "$unnamed" == 0 ]] || echo "  $unnamed unnamed 'error: ...' line(s); every error must print error[<name>]:"
      tail -n 40 "$work/out" | sed 's/^/  | /'
    fi
  done
done <samples.txt

echo "verify-samples: $roots roots, $runs runs, $mismatches mismatches"
[[ "$roots" -gt 0 && "$mismatches" -eq 0 ]]
