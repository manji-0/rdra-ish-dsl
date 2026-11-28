#!/usr/bin/env python3
"""Regenerate the evaluation tables of docs/paper (roadmap #594: T4, T6, T7, T10).

Usage (from the repository root; BIN is a `--features perf` build, see docs/paper/eval/README.md):

  scripts/paper/eval.py ablation  BIN OUT.tsv [SPEC ...]
  scripts/paper/eval.py scaling   BIN OUT.tsv [SPEC ...]   # SPEC without --instance-count
  scripts/paper/eval.py minimal   BIN OUT.tsv              # T4 over the corpus and the failing samples
  scripts/paper/eval.py determinism BIN OUT.tsv [SPEC ...]
  scripts/paper/eval.py table     KIND IN.tsv              # print the Markdown table of a TSV

A SPEC is a path under the repository with the flags of its run, as one argument:
"samples/patent-ops" or "samples/oidc-auth --instance-count 1". Without SPECs each
subcommand takes its default list (DEFAULT_SPECS).

Every run is one `verify` process at RDRA_JOBS=1 (except `determinism`), with RDRA_PERF=1,
under a timeout (RDRA_EVAL_TIMEOUT seconds, default 900) and `--memory` (RDRA_EVAL_MEMORY,
default 12GiB). The script records the run's own user CPU and peak RSS from wait4, the
states and edges of every BFS run from the `perf-run` lines, and each check's verdict word, so
two configurations can be compared verdict by verdict. It uses the standard library only.
"""

import os
import re
import subprocess
import sys
import tempfile
import time

TIMEOUT = int(os.environ.get("RDRA_EVAL_TIMEOUT", "900"))
MEMORY = os.environ.get("RDRA_EVAL_MEMORY", "12GiB")

DEFAULT_SPECS = [
    "samples/ec-site",
    "samples/personal-info",
    "samples/api-contract",
    "samples/incremental-order/step-7-business-rules/src",
    "samples/patent-ops",
    "samples/clinic-ops",
    "samples/food-permit-ops",
    "samples/insurance-claim-ops",
    "samples/oidc-auth --instance-count 1",
]

# The four configurations of the ablation: environment and extra flags.
CONFIGS = [
    ("cone", {"RDRA_ISH_NO_REDUCE": "1"}, ["--symmetry", "never"]),
    ("cone+reduce", {}, ["--symmetry", "never"]),
    ("cone+sym", {"RDRA_ISH_NO_REDUCE": "1"}, ["--symmetry", "always"]),
    ("cone+reduce+sym", {}, ["--symmetry", "always"]),
]

VERDICT = re.compile(r"^(\S+) \[([^\]]*)\]: (ok|counterexample|not decided|tool_error|violated)\b")
ERROR = re.compile(r"^error\[([^\]]+)\]")
RESULT = re.compile(r"^result: (.*)$")
STEPS = re.compile(r"counterexample \((\d+) steps?\)")


def fields(line):
    return dict(f.split("=", 1) for f in line.split()[1:] if "=" in f)


def run(binary, spec, env_extra=None, flags=(), jobs="1"):
    """Runs `verify` once. Returns a dict of measurements, the stdout, and the verdicts."""
    path, *spec_flags = spec.split()
    env = {k: v for k, v in os.environ.items() if not k.startswith("RDRA_ISH_")}
    env.update({"RDRA_JOBS": jobs, "RDRA_PERF": "1"})
    env.update(env_extra or {})
    cmd = [binary, "verify", path, *spec_flags, *flags, "--memory", MEMORY]
    with tempfile.TemporaryFile("w+") as fo, tempfile.TemporaryFile("w+") as fe:
        start = time.monotonic()
        child = subprocess.Popen(cmd, env=env, stdout=fo, stderr=fe, text=True)
        timed_out = False
        # The child's own usage (user CPU, peak RSS), read by wait4 as it is reaped.
        while True:
            pid, status, usage = os.wait4(child.pid, os.WNOHANG)
            if pid:
                break
            if time.monotonic() - start > TIMEOUT:
                child.kill()
                timed_out = True
                _, status, usage = os.wait4(child.pid, 0)
                break
            time.sleep(0.02)
        child.returncode = os.waitstatus_to_exitcode(status)
        wall = time.monotonic() - start
        fo.seek(0)
        fe.seek(0)
        out, err = fo.read(), fe.read()
    return measure(out, err, wall, usage, child.returncode, timed_out)


def measure(out, err, wall, usage, code, timed_out):
    user = usage.ru_utime
    rss_mb = usage.ru_maxrss / 1024  # KiB on Linux
    runs =[fields(l) for l in err.splitlines() if l.startswith("perf-run ")]
    passes = [fields(l) for l in err.splitlines() if l.startswith("perf-pass ")]
    states = sum(int(r["states"]) for r in runs)
    edges = sum(int(r["edges"]) for r in runs)
    largest = max((int(r["states"]) for r in runs), default=0)
    peak_store = max((int(r["peak"]) for r in runs), default=0)
    peak_live = max((float(p.get("peak_live_mb", 0)) for p in passes), default=0.0)
    bfs_cpu = sum(float(r["thread_cpu_ms"]) for r in runs)
    verdicts = {}
    steps = []
    result = "?"
    lines = (out + "\n" + err).splitlines()
    for l in lines:
        m = VERDICT.match(l)
        if m:
            verdicts[m.group(1)] = m.group(3)
        m = ERROR.match(l)
        if m:
            verdicts["error:" + m.group(1)] = "error"
        m = RESULT.match(l)
        if m:
            result = m.group(1).split(" (")[0]
        m = STEPS.search(l)
        if m:
            steps.append(int(m.group(1)))
    if timed_out:
        result = "timeout"
    return {
        "status": "timeout" if timed_out else str(code),
        "result": result,
        "wall_s": round(wall, 2),
        "user_s": round(user, 2),
        "rss_mb": round(rss_mb),
        "bfs_cpu_s": round(bfs_cpu / 1000, 2),
        "runs": len(runs),
        "states": states,
        "edges": edges,
        "largest": largest,
        "peak_store_mb": round(peak_store / 2**20, 1),
        "peak_live_mb": round(peak_live, 1),
        "verdicts": verdicts,
        "steps": steps,
        "stdout": out,
        "text": out + "\n" + err,
    }


COLUMNS = ["spec", "config", "status", "result", "wall_s", "user_s", "bfs_cpu_s", "runs",
           "states", "edges", "largest", "peak_store_mb", "peak_live_mb", "rss_mb", "same_verdicts"]


def write_row(f, row):
    f.write("\t".join(str(row.get(c, "")) for c in COLUMNS) + "\n")
    f.flush()


def verdict_key(v):
    """The verdicts of a run, as a comparable set: a check's word, and its error lines."""
    return tuple(sorted(v.items()))


def ablation(binary, out, specs):
    with open(out, "w") as f:
        f.write("\t".join(COLUMNS) + "\n")
        for spec in specs:
            base = None
            for name, env, flags in CONFIGS:
                m = run(binary, spec, env, flags)
                if base is None:
                    base = m
                same = "-" if m["status"] == "timeout" or base["status"] == "timeout" else (
                    "yes" if same_errors(m, base) and m["result"] == base["result"] else "NO")
                write_row(f, dict(m, spec=spec, config=name, same_verdicts=same))
                print(f"{spec} {name}: {m['result']} {m['states']} states {m['user_s']} s", file=sys.stderr)


def same_errors(a, b):
    """Two runs report the same failing checks and the same overall result."""
    ea = {k for k in a["verdicts"] if k.startswith("error:")}
    eb = {k for k in b["verdicts"] if k.startswith("error:")}
    return ea == eb


def scaling(binary, out, specs, counts=(1, 2, 3, 4)):
    with open(out, "w") as f:
        f.write("\t".join(COLUMNS) + "\n")
        for spec in specs:
            for sym in ("never", "always"):
                for n in counts:
                    m = run(binary, spec, {}, ["--symmetry", sym, "--instance-count", str(n)])
                    write_row(f, dict(m, spec=spec, config=f"n={n} sym={sym}", same_verdicts="-"))
                    print(f"{spec} n={n} {sym}: {m['result']} {m['states']} {m['user_s']} s", file=sys.stderr)
                    if m["status"] == "timeout" or "memory limit" in m["stdout"]:
                        break


CORPUS = "crates/rdra-ish-cli/tests/engine_corpus"
EXPECT = re.compile(r"^// expect (\S+): native=(\w+)")
FLAGS = re.compile(r"^// flags: (.*)$")


def corpus_cases():
    """(path, flags, expected failures) for each corpus case and each failing sample header."""
    cases = []
    for name in sorted(os.listdir(CORPUS)):
        if not name.endswith(".rdra"):
            continue
        p = os.path.join(CORPUS, name)
        fails, flags = [], []
        with open(p) as fh:
            for l in fh:
                m = EXPECT.match(l)
                if m and m.group(2) in ("fail", "counterexample"):
                    fails.append(m.group(1))
                m = FLAGS.match(l)
                if m:
                    flags = m.group(1).split()
        if fails:
            cases.append((p, flags, fails))
    sample_expect = re.compile(r"^// expect verify(.*?): fail (.*)$")
    for root in ("samples/formal-verification", "samples/formal-verification-fail"):
        for name in sorted(os.listdir(root)):
            p = os.path.join(root, name)
            if not name.endswith(".rdra"):
                continue
            with open(p) as fh:
                for l in fh:
                    m = sample_expect.match(l.rstrip())
                    if m:
                        cases.append((p, m.group(1).split(), m.group(2).split()))
    return cases


def strip_instances(flags):
    """The flags without --instance-count and --instances, which `minimal` sets itself."""
    out, skip = [], False
    for f in flags:
        if skip:
            skip = False
            continue
        if f in ("--instance-count", "--instances"):
            skip = True
            continue
        out.append(f)
    return out


def minimal(binary, out, counts=(1, 2, 3)):
    """T4: the least uniform instance count at which each expected counterexample appears.

    Runs each case at its header flags with every --instance-count/--instances removed and
    `--instance-count k` for k = 1, 2, 3, and records for each expected failing check the least
    k whose run reports it, with the counterexample's length in steps at that k.
    """
    with open(out, "w") as f:
        f.write("case\tflags\tcheck\tleast_n\tsteps_at_least\theader_flags\n")
        for path, flags, fails in corpus_cases():
            base = strip_instances(flags)
            found = {}
            for k in counts:
                m = run(binary, " ".join([path, *base]), {}, ["--instance-count", str(k)])
                errs = {e[len("error:"):] for e in m["verdicts"] if e.startswith("error:")}
                for check in fails:
                    if check in errs and check not in found:
                        found[check] = (k, step_of(m["text"], check))
                if all(c in found for c in fails):
                    break
            for check in fails:
                k, steps = found.get(check, (f">{counts[-1]}", "-"))
                f.write(f"{path}\t{' '.join(base)}\t{check}\t{k}\t{steps}\t{' '.join(flags)}\n")
                f.flush()
            print(f"{path}: {found}", file=sys.stderr)


def step_of(stdout, check):
    """The length of the counterexample printed under `error[check]`, or '-'."""
    lines = stdout.splitlines()
    for i, l in enumerate(lines):
        if l.startswith(f"error[{check}]"):
            for l2 in lines[i + 1:i + 12]:
                m = STEPS.search(l2)
                if m:
                    return int(m.group(1))
                if l2.startswith("error[") or VERDICT.match(l2):
                    break
            return 0 if "initial state" in l or "step 0" in l else "-"
    return "-"


def determinism(binary, out, specs, jobs=("1", "2", "4"), repeats=2):
    """T10: the stdout of every run, at each RDRA_JOBS and repeated, against the first one."""
    with open(out, "w") as f:
        f.write("spec\tjobs\trepeat\twall_s\tuser_s\tsame_stdout\n")
        for spec in specs:
            first = None
            for j in jobs:
                for r in range(repeats):
                    m = run(binary, spec, {}, [], jobs=j)
                    if first is None:
                        first = m["stdout"]
                    same = "yes" if m["stdout"] == first else "NO"
                    f.write(f"{spec}\t{j}\t{r}\t{m['wall_s']}\t{m['user_s']}\t{same}\n")
                    f.flush()
                    print(f"{spec} jobs={j} #{r}: {same} {m['wall_s']} s", file=sys.stderr)


def read_tsv(path):
    with open(path) as f:
        head = f.readline().rstrip("\n").split("\t")
        return [dict(zip(head, l.rstrip("\n").split("\t"))) for l in f if l.strip()]


def num(s):
    try:
        return f"{int(s):,}"
    except ValueError:
        return s


def table(kind, path):
    rows = read_tsv(path)
    if kind == "ablation":
        print("| Model | Configuration | Result | States (all runs) | Largest run | Peak RSS (MB) | User CPU (s) | Same verdicts |")
        print("|---|---|---|---:|---:|---:|---:|---|")
        for r in rows:
            print(f"| {r['spec']} | {r['config']} | {r['result']} | {num(r['states'])} | {num(r['largest'])} | "
                  f"{r['rss_mb']} | {r['user_s']} | {r['same_verdicts']} |")
    elif kind == "scaling":
        print("| Model | Run | Result | States (all runs) | Largest run | Peak RSS (MB) | User CPU (s) |")
        print("|---|---|---|---:|---:|---:|---:|")
        for r in rows:
            print(f"| {r['spec']} | {r['config']} | {r['result']} | {num(r['states'])} | {num(r['largest'])} | "
                  f"{r['rss_mb']} | {r['user_s']} |")
    elif kind == "minimal":
        print("| Case | Check | Least n | Steps at least n |")
        print("|---|---|---:|---:|")
        for r in rows:
            print(f"| {os.path.basename(r['case'])} | {r['check']} | {r['least_n']} | {r['steps_at_least']} |")
    elif kind == "determinism":
        print("| Model | RDRA_JOBS | Repeat | Wall (s) | User CPU (s) | Same stdout as first run |")
        print("|---|---:|---:|---:|---:|---|")
        for r in rows:
            print(f"| {r['spec']} | {r['jobs']} | {r['repeat']} | {r['wall_s']} | {r['user_s']} | {r['same_stdout']} |")
    else:
        sys.exit(f"unknown table kind {kind}")


def main(argv):
    if len(argv) < 3:
        sys.exit(__doc__)
    cmd = argv[0]
    if cmd == "table":
        return table(argv[1], argv[2])
    binary = os.path.abspath(argv[1])
    out = argv[2]
    specs = argv[3:] or DEFAULT_SPECS
    if cmd == "ablation":
        ablation(binary, out, specs)
    elif cmd == "scaling":
        scaling(binary, out, specs)
    elif cmd == "minimal":
        minimal(binary, out)
    elif cmd == "determinism":
        determinism(binary, out, specs)
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])
