#!/usr/bin/env python3
"""T16 of #594: what `verify --prove` decides on the samples, as an evaluated heuristic.

Usage:
  scripts/paper/prove-eval.py run BIN OUT.tsv      # z3 on PATH
  scripts/paper/prove-eval.py table IN.tsv

Runs `verify ROOT --prove` at RDRA_JOBS=1 on every root of samples.txt (domain samples at their
default bounds, oidc-auth at 1 instance) and records one row per state rule: proved, unproved
(with the reason: not inductive, z3 gave up, initial state), refuted by BFS, escalated (BFS at
n+1 and n+2), or out of scope, with the run's wall time. Standard library only.
"""

import collections
import os
import re
import subprocess
import sys
import time

LINE = re.compile(r"^(proved|unproved|refuted|escalated|out of scope): (\S+?)(?::|\s|$)(.*)$")


def roots():
    with open("samples.txt") as f:
        rs = [l.strip() for l in f if l.strip() and not l.startswith("#")]
    return [r + (" --instance-count 1" if r == "samples/oidc-auth" else "") for r in rs]


def run(binary, out):
    env = dict(os.environ, RDRA_JOBS="1")
    with open(out, "w") as f:
        f.write("root\trule\tverdict\tdetail\twall_s\n")
        for spec in roots():
            path, *flags = spec.split()
            start = time.monotonic()
            p = subprocess.run([binary, "verify", path, *flags, "--prove", "--memory", "12GiB"],
                               capture_output=True, text=True, env=env, timeout=1800)
            wall = round(time.monotonic() - start, 2)
            n = 0
            for l in (p.stdout + "\n" + p.stderr).splitlines():
                m = LINE.match(l)
                if m:
                    verdict, rule, rest = m.groups()
                    if verdict == "unproved":
                        rest = rest.strip().split(";")[0]
                    f.write(f"{spec}\t{rule}\t{verdict}\t{rest.strip()[:120]}\t{wall}\n")
                    n += 1
            f.flush()
            print(f"{spec}: {n} rules, {wall} s", file=sys.stderr)


def table(path):
    with open(path) as f:
        head = f.readline().rstrip("\n").split("\t")
        rows = [dict(zip(head, l.rstrip("\n").split("\t"))) for l in f if l.strip()]
    by = collections.OrderedDict()
    for r in rows:
        key = r["verdict"] + (" (" + r["detail"] + ")" if r["verdict"] == "unproved" else "")
        by.setdefault(r["root"], collections.Counter())[r["verdict"]] += 1
    kinds = ["proved", "unproved", "refuted", "escalated", "out of scope"]
    walls = {r["root"]: r["wall_s"] for r in rows}
    print("| Root | " + " | ".join(kinds) + " | Wall (s) |")
    print("|---|" + "--:|" * len(kinds) + "--:|")
    tot = collections.Counter()
    for root, c in by.items():
        tot.update(c)
        print(f"| {root} | " + " | ".join(str(c[k]) for k in kinds) + f" | {walls[root]} |")
    print("| **Total** | " + " | ".join(f"**{tot[k]}**" for k in kinds) + " | |")
    print()
    print("| Unproved: reason | Rules |")
    print("|---|--:|")
    reasons = collections.Counter(r["detail"].split(" from ")[0].split(" (")[0] for r in rows if r["verdict"] == "unproved")
    for k, v in reasons.most_common():
        print(f"| {k} | {v} |")


if __name__ == "__main__":
    if sys.argv[1:2] == ["table"]:
        table(sys.argv[2])
    else:
        run(os.path.abspath(sys.argv[2]), sys.argv[3])
