#!/usr/bin/env bash
# Check the sets the code's run-time check of Lemma 5 reads with the Lean checker of
# proofs/lean, on every split of the samples, the domain samples and the engine corpus cases
# that build a model (the walk of `every_split_passes_its_check`).
#
#   scripts/check-lean-certificates.sh [--profile PROFILE]
#
# PROFILE is the cargo profile the dump's test binary is built with (default: `test`).
#
# 1. The ignored Rust test `dump_lemma5_certificates` writes one certificate per distinct
#    split: each action's reads and writes, `monotone_vars`, and the split's sets.
# 2. `lake build` checks the proof that the checker is sound (`Lemma5Check.lean`) and builds
#    the checker `lemma5-check`, which runs on the dump and must accept every certificate.
# 3. Faults planted in copies of the dump, one per clause, must each be rejected, with the
#    clause named.
#
# Needs `lake` on PATH, as scripts/check-lean-proofs.sh does.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
certs="$work/certs.jsonl"
profile=test
if [[ "${1:-}" == --profile ]]; then
  profile="$2"
fi

cd "$root"
RDRA_LEAN_CERTS="$certs" cargo test -q --profile "$profile" -p rdra-ish-core --lib \
  lifecycle::tests::reduce::dump_lemma5_certificates -- --ignored --exact --nocapture

cd "$root/proofs/lean"
# The linker's warnings about libgmp's macOS version say nothing about the build, so the
# log is shown without them.
if ! lake build RdraIsh.Lemma5Check lemma5-check >"$work/lake.log" 2>&1; then
  grep -v 'ld64.lld: warning' "$work/lake.log" >&2
  echo "check-lean-certificates: lake build failed" >&2
  exit 1
fi
grep -v 'ld64.lld: warning' "$work/lake.log" | tail -1
check=./.lake/build/bin/lemma5-check
echo "certificates: $(wc -l <"$certs" | tr -d ' ') lines, $(wc -c <"$certs" | tr -d ' ') bytes"
"$check" "$certs"

# Each plant changes one certificate of a copy of the dump so that it breaks one clause, and
# writes the name `lemma5-check` must report for it.
python3 -I - "$certs" "$work" <<'PY'
import json
import sys

certs, work = sys.argv[1], sys.argv[2]
lines = [json.loads(line) for line in open(certs)]


def plant_a(c):
    """(a) a var an S_a action writes moved from S_v to T_v."""
    for a in c["sa"]:
        for v in c["wr"][a]:
            c["s"].remove(v)
            c["t"].append(v)
            return "a"


def plant_b(c):
    """(b) an S_v var a T_a action reads dropped from obs."""
    for a in c["ta"]:
        for v in c["rd"][a]:
            if v in c["obs"]:
                c["obs"].remove(v)
                return "b reads"


def plant_c(c):
    """(c) an input taken away from an S_a action that reads it outside its fold."""
    for v in c["inputs"]:
        if any(v in c["rd"][a] and v not in c["folded"][a] for a in c["sa"]):
            c["inputs"].remove(v)
            return "c"


def plant_d(c):
    """(d) a T_v var that is not monotone made an input."""
    for v in c["t"]:
        if v not in c["mono"]:
            c["inputs"].append(v)
            return "d"


def plant_e(c):
    """(e) a kept action that writes moved from T_a to S_a."""
    for a in c["keep"]:
        if c["wr"][a]:
            c["ta"].remove(a)
            c["sa"].append(a)
            return "e kept"


def plant_b_writes(c):
    """(b) a var a T_a action writes moved from T_v to S_v."""
    for a in c["ta"]:
        for v in c["wr"][a]:
            c["t"].remove(v)
            c["s"].append(v)
            return "b writes"


def plant_obs(c):
    """A T_v var put in obs."""
    for v in c["t"]:
        c["obs"].append(v)
        return "obs in S"


def plant_seed(c):
    """(e) an S_v var a seed reads dropped from obs."""
    for r in c["seeds"]:
        for v in r:
            if v in c["obs"]:
                c["obs"].remove(v)
                return "e seeds"


def plant_inputs(c):
    """An S_v var made an input."""
    for v in c["s"]:
        c["inputs"].append(v)
        return "inputs in T"


def plant_sides(c):
    """A T_v var put in S_v too."""
    for v in c["t"]:
        c["s"].append(v)
        return "sides"


def plant_range(c):
    """A var id past the last one put in T_v."""
    c["t"].append(c["nv"])
    return "well-formed"


plants = [plant_a, plant_b, plant_b_writes, plant_obs, plant_c, plant_d, plant_seed, plant_e,
          plant_inputs, plant_sides, plant_range]
for n, plant in enumerate(plants):
    planted = [json.loads(json.dumps(c)) for c in lines]
    part = next((p for p in map(plant, planted) if p is not None), None)
    if part is None:
        sys.exit(f"check-lean-certificates: no certificate to plant {plant.__name__} in")
    with open(f"{work}/planted-{n}.jsonl", "w") as f:
        f.writelines(json.dumps(c) + "\n" for c in planted)
    with open(f"{work}/planted-{n}.part", "w") as f:
        f.write(part)
PY
for planted in "$work"/planted-*.jsonl; do
  part=$(cat "${planted%.jsonl}.part")
  if out=$("$check" "$planted" 2>&1); then
    echo "check-lean-certificates: a planted fault ($part) was accepted" >&2
    exit 1
  fi
  rejected=$(grep 'rejected: \[' <<<"$out" || true)
  if [[ $(wc -l <<<"$rejected") != 1 || "$rejected" != *"[$part"* && "$rejected" != *", $part"* ]]; then
    echo "check-lean-certificates: the planted fault ($part) was not the one rejection:" >&2
    echo "$out" >&2
    exit 1
  fi
  echo "planted ($part): $rejected"
done
echo "check-lean-certificates: every certificate accepted, every planted fault rejected"
