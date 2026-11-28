#!/usr/bin/env python3
"""T8 of #594: per-cone states and times of the native checker and TLC, from `verify --with-tlc`.

Usage:
  scripts/paper/tlc-local.py run BIN OUT.tsv [SPEC ...]   # tlc on PATH
  scripts/paper/tlc-local.py table IN.tsv

A SPEC is a path with flags, as one argument ("samples/oidc-auth --instance-count 1"). Each
run is `verify SPEC --with-tlc --memory 12GiB` (BFS takes half, TLC the rest). The script pairs
each native line `Cone_X [...]: <verdict> (<n> states...` with TLC's line
`tlc: Cone_X [...]: <verdict> (<n> states, <t> s, ...)` and writes one row per pair, with the
run's wall time. It also runs the same SPEC without TLC and records that wall time, so the cost
of TLC can be read beside the native checker's. Standard library only.
"""

import os
import re
import subprocess
import sys
import time

DEFAULT = [
    "samples/ec-site",
    "samples/personal-info",
    "samples/incremental-order/step-7-business-rules/src",
    "samples/patent-ops",
    "samples/clinic-ops",
    "samples/food-permit-ops",
    "samples/insurance-claim-ops",
    "samples/oidc-auth --instance-count 1",
]

NATIVE = re.compile(r"^((?:Cone|RdraSpec)_\S+) \[[^\]]*\]: (ok|counterexample|not decided|tool_error)[^(]*\((\d+) states")
TLC = re.compile(r"^tlc: ((?:Cone|RdraSpec)_\S+) \[[^\]]*\]: (ok|counterexample|not decided|tool_error)[^(]*\((\d+) states.*?, ([\d.]+) s, ")
MODEL = re.compile(r"^tlc: (StateModel_\S+) \((\d+) states\): (.*)$")
RESULT = re.compile(r"^(result|tlc_result): (\S+)")


def run(binary, spec, with_tlc):
    path, *flags = spec.split()
    cmd = [binary, "verify", path, *flags, "--memory", "12GiB"] + (["--with-tlc"] if with_tlc else [])
    start = time.monotonic()
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=3600)
    return p.stdout + "\n" + p.stderr, round(time.monotonic() - start, 2)


def main(argv):
    if argv[:1] == ["table"]:
        return table(argv[1])
    binary, out = os.path.abspath(argv[1]), argv[2]
    specs = argv[3:] or DEFAULT
    with open(out, "w") as f:
        f.write("spec\tcone\tnative_verdict\tnative_states\ttlc_verdict\ttlc_states\ttlc_s\trun_wall_s\tnative_only_wall_s\tresult\ttlc_result\n")
        for spec in specs:
            text, wall = run(binary, spec, True)
            _, native_wall = run(binary, spec, False)
            native, tlc, results = {}, {}, {}
            for line in text.splitlines():
                if m := NATIVE.match(line):
                    native[m.group(1)] = (m.group(2), m.group(3))
                elif m := TLC.match(line):
                    tlc[m.group(1)] = (m.group(2), m.group(3), m.group(4))
                elif m := MODEL.match(line):
                    tlc[m.group(1)] = ("state model", m.group(2), "-")
                elif m := RESULT.match(line):
                    results[m.group(1)] = m.group(2)
            for cone in list(native) + [c for c in tlc if c not in native]:
                nv, ns = native.get(cone, ("-", "-"))
                tv, ts, tt = tlc.get(cone, ("-", "-", "-"))
                f.write(f"{spec}\t{cone}\t{nv}\t{ns}\t{tv}\t{ts}\t{tt}\t{wall}\t{native_wall}\t"
                        f"{results.get('result', '?')}\t{results.get('tlc_result', '?')}\n")
            f.flush()
            print(f"{spec}: {len(native)} native, {len(tlc)} tlc lines, {wall} s with TLC, {native_wall} s without, "
                  f"result {results.get('result')} tlc_result {results.get('tlc_result')}", file=sys.stderr)


def table(path):
    with open(path) as f:
        head = f.readline().rstrip("\n").split("\t")
        rows = [dict(zip(head, l.rstrip("\n").split("\t"))) for l in f if l.strip()]
    print("| Model | Cone | Native | Native states | TLC | TLC states | TLC time (s) |")
    print("|---|---|---|--:|---|--:|--:|")
    for r in rows:
        print(f"| {r['spec']} | {r['cone']} | {r['native_verdict']} | {r['native_states']} | {r['tlc_verdict']} | "
              f"{r['tlc_states']} | {r['tlc_s']} |")


if __name__ == "__main__":
    main(sys.argv[1:])
