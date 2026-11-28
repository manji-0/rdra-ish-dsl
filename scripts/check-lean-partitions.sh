#!/usr/bin/env bash
# Check the partitions `Classes::of_graph` computes, and the shape of the extended
# sub-lifecycle X they partition, with the Lean checker of proofs/lean, on every flat run of
# X the reduction of the samples', the domain samples' and the engine corpus cases' cones
# partitions (the walk of `every_quotient_passes_the_checks_of_its_hypotheses`, at
# `Bounds::STATES` and `Bounds::VERIFY`).
#
#   scripts/check-lean-partitions.sh [--profile PROFILE]
#
# PROFILE is the cargo profile the dump's test binary is built with (default: `test`).
# scripts/check-lean-certificates.sh runs this script after its own checks.
#
# 1. The ignored Rust test `dump_partition_certificates` writes one record per distinct
#    flat run: the numbered run (each state's obs, S and input values, the silent edges,
#    the rises, the initial states), each state's class and each class's representative.
#    It counts the runs it leaves out, by reason.
# 2. `lake build` checks the proof that the checker is sound (`PartitionCheck.lean`) and
#    builds the checker `partition-check`, which runs on the dump and must accept every
#    record.
# 3. Faults planted in copies of the dump, one per part of the check, must each be
#    rejected, with the part named.
#
# Needs `lake` on PATH, as scripts/check-lean-proofs.sh does.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
runs="$work/runs.txt"
profile=test
if [[ "${1:-}" == --profile ]]; then
  profile="$2"
fi

cd "$root"
RDRA_LEAN_RUNS="$runs" cargo test -q --profile "$profile" -p rdra-ish-core --lib \
  lifecycle::tests::reduce::dump_partition_certificates -- --ignored --exact --nocapture

cd "$root/proofs/lean"
# The linker's warnings about libgmp's macOS version say nothing about the build, so the
# log is shown without them.
if ! lake build RdraIsh.PartitionCheck partition-check >"$work/lake.log" 2>&1; then
  grep -v 'ld64.lld: warning' "$work/lake.log" >&2
  echo "check-lean-partitions: lake build failed" >&2
  exit 1
fi
grep -v 'ld64.lld: warning' "$work/lake.log" | tail -1
check=./.lake/build/bin/partition-check
echo "runs: $(grep -c '^c ' "$runs") records, $(wc -c <"$runs" | tr -d ' ') bytes"
"$check" "$runs"

# Each plant changes one record of a copy of the dump so that it breaks one part, and
# writes the name `partition-check` must report for it. A plant takes the first record,
# smallest first, where its change breaks the part, by the plant's own reading of it.
python3 -I - "$runs" "$work" <<'PY'
import sys
from collections import defaultdict

runs, work = sys.argv[1], sys.argv[2]
records = []
for line in open(runs):
    line = line.rstrip("\n")
    if line.startswith("c "):
        records.append({"c": line[2:]})
    else:
        tag, _, rest = line.partition(" ")
        records[-1][tag] = [int(x) for x in rest.split()]


def edges(r, tag):
    a = r[tag]
    return [(a[i], a[i + 1]) for i in range(0, len(a), 2)]


def put_edges(r, tag, es):
    r[tag] = [x for e in es for x in e]


def members(r):
    m = defaultdict(list)
    for i, b in enumerate(r["b"]):
        m[b].append(i)
    return m


def b2_holds(r):
    """Each state of a class reaches, through the class, each class a silent edge from the
    class enters."""
    blk = r["b"]
    succ = defaultdict(list)
    exits = defaultdict(set)
    for p, t in edges(r, "e"):
        if p != t:
            succ[p].append(t)
            if blk[p] != blk[t]:
                exits[blk[p]].add(blk[t])
    for b, ms in members(r).items():
        for p in ms:
            seen, stack, got = {p}, [p], set()
            while stack:
                x = stack.pop()
                for t in succ[x]:
                    if blk[t] != b:
                        got.add(blk[t])
                    elif t not in seen:
                        seen.add(t)
                        stack.append(t)
            if got != exits[b]:
                return False
    return True


def entered(r):
    out = defaultdict(set)
    for p, y in edges(r, "u"):
        out[p].add(r["b"][y])
    return out


def b3_holds(r):
    ent = entered(r)
    return all(ent[i] == ent[r["r"][b]] for i, b in enumerate(r["b"]))


def plant_b2(r):
    """Two classes with the same obs and input bytes merged, where the merge splits."""
    if r["n"][0] > 2000:
        return None
    rep, nb = r["r"], r["n"][1]
    key = lambda c: (r["o"][rep[c]], r["i"][rep[c]])
    for b in range(nb):
        for c in range(b + 1, nb):
            if key(b) != key(c):
                continue
            last = nb - 1
            blk = [b if x == c else x for x in r["b"]]
            if c != last:
                blk = [c if x == last else x for x in blk]
            reps = rep[:]
            if c != last:
                reps[c] = rep[last]
            reps.pop()
            trial = dict(r, b=blk, r=reps, n=[r["n"][0], nb - 1])
            if not b2_holds(trial):
                r.update(trial)
                return "b2"
    return None


def plant_b3(r):
    """A rise added from a state that shares its class to a state of the same S, in a
    class the other states of its class do not enter."""
    ms = members(r)
    rises = edges(r, "u")
    for k, (p, y) in enumerate(rises):
        if len(ms[r["b"][p]]) < 2:
            continue
        for y2 in range(r["n"][0]):
            if r["s"][y2] == r["s"][p] and r["b"][y2] != r["b"][y]:
                trial = dict(r)
                put_edges(trial, "u", rises + [(p, y2)])
                if not b3_holds(trial):
                    r.update(trial)
                    return "b3"
    return None


def plant_silent(r):
    """A silent edge sent to a state with other input bytes."""
    silent = edges(r, "e")
    for k, (p, t) in enumerate(silent):
        for t2 in range(r["n"][0]):
            if r["i"][t2] != r["i"][p]:
                put_edges(r, "e", silent[:k] + [(p, t2)] + silent[k + 1:])
                return "silent keeps inputs"
    return None


def plant_rise(r):
    """A rise sent to a state of the class it entered, but with another S."""
    rises = edges(r, "u")
    ms = members(r)
    for k, (p, y) in enumerate(rises):
        for y2 in ms[r["b"][y]]:
            if r["s"][y2] != r["s"][p]:
                put_edges(r, "u", rises[:k] + [(p, y2)] + rises[k + 1:])
                return "rise keeps S"
    return None


def plant_rep(r):
    """A class's representative moved to a state of another class."""
    if r["n"][1] < 2:
        return None
    r["r"][0] = r["r"][1]
    return "rep"


def plant_b1(r):
    """A state that is not its class's representative given an obs value of its own."""
    reps = set(r["r"])
    for i in range(r["n"][0]):
        if i not in reps:
            r["o"][i] = max(r["o"]) + 1
            return "b1"
    return None


def plant_reach(r):
    """The initial states dropped."""
    if not r["z"]:
        return None
    r["z"] = []
    return "reach"


def plant_range(r):
    """A class number past the last one."""
    r["b"][0] = r["n"][1]
    return "well-formed"


plants = [plant_b1, plant_b2, plant_b3, plant_rep, plant_silent, plant_rise, plant_reach,
          plant_range]
order = sorted(range(len(records)), key=lambda k: records[k]["n"][0])
for n, plant in enumerate(plants):
    part = None
    for k in order:
        planted = {t: (v[:] if isinstance(v, list) else v) for t, v in records[k].items()}
        part = plant(planted)
        if part is not None:
            break
    if part is None:
        sys.exit(f"check-lean-partitions: no record to plant {plant.__name__} in")
    with open(f"{work}/planted-{n}.txt", "w") as f:
        for j, r in enumerate(records):
            r = planted if j == k else r
            f.write(f"c {r['c']}\n")
            for tag in "nosibrzeu":
                f.write(" ".join([tag] + [str(x) for x in r[tag]]) + "\n")
    with open(f"{work}/planted-{n}.part", "w") as f:
        f.write(part)
PY
for planted in "$work"/planted-*.txt; do
  part=$(cat "${planted%.txt}.part")
  if out=$("$check" "$planted" 2>&1); then
    echo "check-lean-partitions: a planted fault ($part) was accepted" >&2
    exit 1
  fi
  rejected=$(grep 'rejected: \[' <<<"$out" || true)
  if [[ $(wc -l <<<"$rejected") != 1 || "$rejected" != *"[$part"* && "$rejected" != *", $part"* ]]; then
    echo "check-lean-partitions: the planted fault ($part) was not the one rejection:" >&2
    echo "$out" >&2
    exit 1
  fi
  echo "planted ($part): $rejected"
done
echo "check-lean-partitions: every record accepted, every planted fault rejected"
