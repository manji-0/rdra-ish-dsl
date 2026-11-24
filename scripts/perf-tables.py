#!/usr/bin/env python3
"""Run `verify` on samples with a `perf` build and print where the time and memory go (#296).

Usage:
  cargo build --release -p rdra-ish-cli --features perf
  scripts/perf-tables.py target/release/rdra-ish [SAMPLE ...]
  scripts/perf-tables.py --from FILE [FILE ...]

A SAMPLE is a directory under samples/ with the flags of its run, as one argument:
`patent-ops` or `"oidc-auth --instance-count 1 --deny-undecided"`. Without one, the five runs
of #296 are measured. Each run is `RDRA_PERF=1 RDRA_JOBS=1`, one at a time; `RDRA_PERF=2`
in the environment passes through and adds the costly columns. `--from` reads the stderr of
runs made by hand instead.

Prints per run the totals, the system's guard conjuncts by what #253 makes of them and the
evaluator calls per expanded state (#300), (instructions retired where macOS `/usr/bin/time -l` reports them,
user CPU, peak RSS, BFS runs, states, edges), the passes, the BFS CPU per pass and label, and
the largest runs with the bytes they keep per state.
"""

import collections
import os
import platform
import re
import subprocess
import sys
import tempfile

DEFAULT = [
    "patent-ops",
    "food-permit-ops",
    "clinic-ops",
    "insurance-claim-ops",
    "oidc-auth --instance-count 1 --deny-undecided",
]
MIB = 1024 * 1024


def fields(line):
    """The key=value fields of a perf line."""
    return dict(f.split("=", 1) for f in line.split()[1:] if "=" in f)


def run(binary, sample):
    """Runs one sample; returns its stderr lines and its totals."""
    name, *flags = sample.split()
    env = dict(os.environ, RDRA_JOBS="1")
    env.setdefault("RDRA_PERF", "1")
    cmd = [binary, "verify", f"samples/{name}", *flags]
    totals = {}
    timing = None
    if platform.system() == "Darwin" and os.path.exists("/usr/bin/time"):
        timing = tempfile.NamedTemporaryFile(suffix=".time", delete=False)
        timing.close()
        cmd = ["/usr/bin/time", "-l", "-o", timing.name, *cmd]
    child = subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
    err = child.stderr.read()
    # The run's own usage, the command it waited for included.
    _, status, usage = os.wait4(child.pid, 0)
    totals["status"] = os.waitstatus_to_exitcode(status)
    totals["user_s"] = usage.ru_utime
    # ru_maxrss is in bytes on macOS and in KiB on Linux.
    rss = usage.ru_maxrss * (1 if platform.system() == "Darwin" else 1024)
    totals["rss_mb"] = rss / MIB
    if timing is not None:
        with open(timing.name) as f:
            m = re.search(r"(\d+)\s+instructions retired", f.read())
        os.unlink(timing.name)
        if m:
            totals["instructions"] = int(m.group(1))
    return err.splitlines(), totals


def report(title, lines, totals):
    passes = [fields(l) for l in lines if l.startswith("perf-pass ")]
    runs = [fields(l) for l in lines if l.startswith("perf-run ")]
    print(f"== {title}")
    states = sum(int(r["states"]) for r in runs)
    edges = sum(int(r["edges"]) for r in runs)
    parts = []
    if "instructions" in totals:
        parts.append(f"instructions {totals['instructions'] / 1e9:.1f} G")
    if "user_s" in totals:
        parts.append(f"user {totals['user_s']:.2f} s")
    if "rss_mb" in totals:
        parts.append(f"peak RSS {totals['rss_mb']:.0f} MB")
    if "status" in totals:
        parts.append(f"exit {totals['status']}")
    parts.append(f"{len(runs)} BFS runs, {states:,} states, {edges:,} edges")
    print("  totals: " + ", ".join(parts))
    if not passes and not runs:
        print("  no perf lines: is the binary built with --features perf?")
        return
    for l in lines:
        if l.startswith("perf-guards "):
            print("  guards of the system: " + l.split(" ", 1)[1])
        elif l.startswith("perf-evals "):
            d = fields(l)
            n = max(int(d.pop("expanded")), 1)
            per = ", ".join(f"{k} {int(v) / n:.2f}" for k, v in d.items())
            print(f"  eval calls per expanded state ({n:,} expanded): {per}")
    print("  passes:")
    for p in passes:
        print(
            "    %-15s wall %9s ms  cpu %9s ms  allocs %10s  alloc %9s MB  peak live %8s MB  pool peak %8s MB"
            % (p["name"], p["wall_ms"], p["cpu_ms"], p["allocs"], p["alloc_mb"], p["peak_live_mb"], p["pool_peak_mb"])
        )
    groups = collections.OrderedDict()
    for r in runs:
        key = (r["pass"] or "-", r["label"] or "-", r["reduced"], r["early"])
        g = groups.setdefault(key, [0, 0, 0, 0.0, 0])
        g[0] += 1
        g[1] += int(r["states"])
        g[2] += int(r["edges"])
        g[3] += float(r["thread_cpu_ms"])
        g[4] = max(g[4], int(r["peak"]))
    cpu = sum(g[3] for g in groups.values()) or 1.0
    print("  BFS thread CPU by pass:")
    for (pas, label, reduced, early), (n, st, ed, ms, peak) in groups.items():
        print(
            "    pass=%-14s label=%-2s reduced=%-5s early=%-5s runs=%4d states=%11s edges=%12s cpu=%9.1f ms (%5.1f%%) largest peak %7.1f MB"
            % (pas, label, reduced, early, n, f"{st:,}", f"{ed:,}", ms, 100 * ms / cpu, peak / MIB)
        )
    print("  largest runs:")
    for r in sorted(runs, key=lambda r: -int(r["states"]))[:6]:
        n, e, kept = int(r["states"]), int(r["edges"]), int(r["kept"])
        ms = float(r["thread_cpu_ms"])
        extra = ""
        if "shash" in r:
            extra = " hash=%s/%s/%s" % (r["shash"][:8], r["ehash"][:8], r["bhash"][:8])
        print(
            "    pass=%s label=%s reduced=%s states=%s edges=%s (%.2f/state) width=%s B bits=%s exact, %s packed kept %.1f MB (%.1f B/state) cpu %.1f ms (%.0f ns/state, %.0f ns/edge)%s"
            % (r["pass"] or "-", r["label"] or "-", r["reduced"], f"{n:,}", f"{e:,}", e / max(n, 1), r["width"],
               r["bits_exact"], r["bits_packed"], kept / MIB, kept / max(n, 1), ms, ms * 1e6 / max(n, 1),
               ms * 1e6 / max(e, 1), extra)
        )


def main(argv):
    if argv[:1] == ["--from"]:
        for path in argv[1:]:
            with open(path) as f:
                report(path, f.read().splitlines(), {})
        return 0
    if not argv or argv[0].startswith("-"):
        print(__doc__, file=sys.stderr)
        return 2
    binary = os.path.abspath(argv[0])
    os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
    for sample in argv[1:] or DEFAULT:
        lines, totals = run(binary, sample)
        report(sample, lines, totals)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
