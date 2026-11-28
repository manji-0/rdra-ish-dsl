#!/usr/bin/env python3
"""Turn the CI log lines of docs/paper/eval/data/tlc-ci-lines.txt into the tables of
docs/paper/eval/tlc-baseline.md.

  python3 -I scripts/paper/tlc-baseline.py [LINES]           print the tables
  python3 -I scripts/paper/tlc-baseline.py [LINES] --check MD  fail unless MD holds them

The tables sit in MD between `<!-- tlc-baseline.py: begin -->` and
`<!-- tlc-baseline.py: end -->`. LINES defaults to docs/paper/eval/data/tlc-ci-lines.txt.
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_LINES = ROOT / "docs/paper/eval/data/tlc-ci-lines.txt"
BEGIN = "<!-- tlc-baseline.py: begin -->"
END = "<!-- tlc-baseline.py: end -->"

HEADER = re.compile(
    r"^## run (\d+) commit ([0-9a-f]+) started (\S+) job (\S+) (\d+) step (\S+)$")
# verify-samples.sh: printf '%-8s %-56s %-22s %7ss %6s MB\n' verdict root label wall rss
SAMPLE = re.compile(r"^(ok|MISMATCH)\s+(\S+)\s+(check|verify)(.*?)\s+([\d.]+)s\s+(\d+) MB$")
SEED = re.compile(r"^seed (\d+): (\S+) \(([\d.]+) s\)$")
TEST = re.compile(r"^test (\S+) \.\.\. (\S+)$")
RESULT = re.compile(r"^test result: (\S+)\. (\d+) passed; (\d+) failed; (\d+) ignored;.* finished in ([\d.]+)s$")


def parse(path):
    """Sections keyed by (run, step), in file order, each with its run's metadata."""
    sections = {}
    order = []
    current = None
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        m = HEADER.match(line)
        if m:
            run, commit, started, job, job_id, step = m.groups()
            current = {"run": run, "commit": commit, "started": started, "job": job,
                       "job_id": job_id, "step": step, "lines": []}
            sections[(run, step)] = current
            if run not in order:
                order.append(run)
            continue
        if current is not None and line and not line.startswith("#"):
            current["lines"].append(line)
    return sections, order


def samples(section):
    rows = []
    summary = None
    for line in section["lines"]:
        m = SAMPLE.match(line)
        if m:
            verdict, root, cmd, rest, wall, rss = m.groups()
            flags = rest.strip()
            native = flags.endswith("(native)")
            if native:
                flags = flags[: -len("(native)")].strip()
            rows.append({"verdict": verdict, "root": root, "cmd": cmd, "flags": flags,
                         "native": native, "wall": float(wall), "rss": int(rss)})
        elif line.startswith("verify-samples:"):
            summary = line
    return rows, summary


def seeds(section):
    rows = []
    summary = None
    for line in section["lines"]:
        m = SEED.match(line)
        if m:
            rows.append((int(m.group(1)), m.group(2), float(m.group(3))))
        elif line.startswith("{"):
            summary = json.loads(line)
    return rows, summary


def tests(section):
    rows = []
    result = None
    for line in section["lines"]:
        m = TEST.match(line)
        if m:
            rows.append(m.groups())
        m = RESULT.match(line)
        if m:
            result = m.groups()
    return rows, result


def short(run, sections):
    sec = next(s for (r, _), s in sections.items() if r == run)
    return f"{run} (`{sec['commit'][:7]}`)"


def tables(path):
    sections, order = parse(path)
    out = []

    # Table 1: the verify runs with TLC beside the native checker.
    per_run = {run: samples(sections[(run, "verify-samples")]) for run in order}
    tlc_keys = []
    for run in order:
        for row in per_run[run][0]:
            key = (row["root"], row["flags"])
            if row["cmd"] == "verify" and not row["native"] and key not in tlc_keys:
                tlc_keys.append(key)
    head = "| Root | Flags beyond the root | Agrees with header |"
    rule = "|---|---|---|"
    for run in order:
        head += f" Wall s, run {short(run, sections)} | Peak RSS MB |"
        rule += "---:|---:|"
    out += ["Table 1. `verify --with-tlc` runs of `verify-samples.sh` (native checker and TLC in "
            "one process tree; wall time and peak RSS cover both).", "", head, rule]
    totals = {run: 0.0 for run in order}
    for key in tlc_keys:
        cells = []
        verdicts = set()
        for run in order:
            row = next((r for r in per_run[run][0] if r["cmd"] == "verify" and not r["native"]
                        and (r["root"], r["flags"]) == key), None)
            if row is None:
                cells += ["n/a", "n/a"]
                continue
            verdicts.add(row["verdict"])
            totals[run] += row["wall"]
            cells += [f"{row['wall']:.2f}", str(row["rss"])]
        agree = "yes" if verdicts == {"ok"} else "no"
        root, flags = key
        out.append(f"| {root} | {('`' + flags + '`') if flags else '(header default)'} | {agree} | "
                   + " | ".join(cells) + " |")
    out.append("| **Total** | | | " + " | ".join(f"**{totals[r]:.2f}** | " for r in order) + "|")
    out.append("")

    # Table 2: run counts of the step.
    out += ["Table 2. All runs of the step.", "",
            "| CI run | Roots | `verify` runs with TLC | `verify` runs native only | `check` runs | "
            "Mismatches | Summary line |",
            "|---|---:|---:|---:|---:|---:|---|"]
    for run in order:
        rows, summary = per_run[run]
        with_tlc = sum(1 for r in rows if r["cmd"] == "verify" and not r["native"])
        native = sum(1 for r in rows if r["cmd"] == "verify" and r["native"])
        checks = sum(1 for r in rows if r["cmd"] == "check")
        mism = sum(1 for r in rows if r["verdict"] != "ok")
        nroots = len({r["root"] for r in rows})
        out.append(f"| {short(run, sections)} | {nroots} | {with_tlc} | {native} | {checks} | "
                   f"{mism} | `{summary}` |")
    out.append("")

    # Table 3: random models, reduced and unreduced, each with TLC beside it.
    out += ["Table 3. `reduction-diff.py run --with-tlc` on the 45 fixed main seeds.", "",
            "| CI run | Seeds compared | Seeds that reduce a cone | Nested | At 3 instances | "
            "`same` | Diverged | Disagreed (`EngineDisagreement`) | Sum of seed wall s | "
            "Longest seed s |",
            "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|"]
    for run in order:
        rows, s = seeds(sections[(run, "reduction-diff-tlc")])
        same = sum(1 for _, v, _ in rows if v == "same")
        total = sum(w for _, _, w in rows)
        longest = max(rows, key=lambda r: r[2])
        nested = s.get("nested", "n/a")
        out.append(f"| {short(run, sections)} | {s['compared']} | {s['reduced']} | {nested} | "
                   f"{s['three_instances']} | {same} | {len(s['diverged'])} | "
                   f"{len(s['disagreed'])} | {total:.1f} | {longest[2]:.1f} (seed {longest[0]}) |")
    out.append("")

    # Table 4: the TLC test binaries of the engine-equivalence job.
    out += ["Table 4. TLC test binaries of the `engine-equivalence` job.", "",
            "| CI run | Test binary | Passed | Failed | Ignored | Wall s |",
            "|---|---|---:|---:|---:|---:|"]
    for run in order:
        for step in ("engine_equivalence", "engine_corpus"):
            sec = sections.get((run, step))
            if sec is None:
                continue
            _, result = tests(sec)
            status, passed, failed, ignored, wall = result
            out.append(f"| {short(run, sections)} | `{step}` | {passed} | {failed} | {ignored} | "
                       f"{float(wall):.2f} |")
    return "\n".join(out) + "\n"


def main(argv):
    args = list(argv)
    check = None
    if "--check" in args:
        i = args.index("--check")
        check = Path(args[i + 1])
        del args[i:i + 2]
    path = Path(args[0]) if args else DEFAULT_LINES
    text = tables(path)
    if check is None:
        sys.stdout.write(text)
        return 0
    doc = check.read_text(encoding="utf-8")
    try:
        block = doc.split(BEGIN, 1)[1].split(END, 1)[0]
    except IndexError:
        print(f"tlc-baseline: {check} has no {BEGIN} ... {END} block", file=sys.stderr)
        return 1
    if block.strip("\n") != text.strip("\n"):
        print(f"tlc-baseline: the tables in {check} differ from what {path} gives",
              file=sys.stderr)
        return 1
    print(f"tlc-baseline: {check} matches {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
