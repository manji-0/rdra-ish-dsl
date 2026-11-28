#!/usr/bin/env python3
"""Print the summary tables of docs/paper/evidence/defects.tsv as Markdown.

Usage: python3 -I scripts/paper/defect-summary.py [--table] [TSV]

Without --table it prints the summary tables that docs/paper/evidence/defects.md
carries under "Summary". With --table it prints the full table of that document
instead. TSV defaults to docs/paper/evidence/defects.tsv beside this script's
repository root.
"""

import csv
import pathlib
import sys

CLASSES = [
    ("1", "1 Lean proof"),
    ("2", "2 Paper proof (theory write-up or its audit)"),
    ("3", "3 Reference interpreter"),
    ("4", "4 TLC differential / engine corpus"),
    ("5", "5 Mutation run"),
    ("6", "6 Run-time self-certification"),
    ("7", "7 Shadow twin"),
    ("8", "8 Kani / Miri / overflow checks"),
    ("9", "9 Random-model reduction-diff"),
    ("10", "10 Test, user report, review, other"),
    ("unstated", "Unstated"),
]
EFFECTS = [
    "wrong ok",
    "wrong counterexample",
    "wrong verdict",
    "crash",
    "tool_error",
    "missed check",
    "other",
    "unstated",
]
COMPONENTS = [
    "builder",
    "cone",
    "reduction",
    "symmetry",
    "BFS/liveness",
    "prover/z3",
    "TLA+ emit",
    "other",
]
VERDICT = {"wrong ok", "wrong counterexample", "wrong verdict"}
COLUMNS = [
    "id", "ref", "version", "date", "component", "effect", "shown",
    "class", "subclass", "description", "evidence",
]


def load(path):
    with open(path, encoding="utf-8", newline="") as f:
        rows = list(csv.DictReader(f, delimiter="\t", quoting=csv.QUOTE_NONE))
    for r in rows:
        if list(r) != COLUMNS:
            sys.exit(f"{path}: columns are {list(r)}, expected {COLUMNS}")
        if r["class"] not in dict(CLASSES):
            sys.exit(f"{r['id']}: unknown class {r['class']!r}")
        if r["component"] not in COMPONENTS:
            sys.exit(f"{r['id']}: unknown component {r['component']!r}")
        kind = "claim" if r["id"].startswith("C") else "defect"
        if (r["effect"] == "claim") != (kind == "claim"):
            sys.exit(f"{r['id']}: a C row has effect claim, and only a C row has")
        if kind == "defect" and r["effect"] not in EFFECTS:
            sys.exit(f"{r['id']}: unknown effect {r['effect']!r}")
        r["kind"] = kind
    return rows


def table(header, body, labels=1):
    lines = ["| " + " | ".join(header) + " |",
             "|" + "|".join("---" if i < labels else "--:" for i in range(len(header))) + "|"]
    lines += ["| " + " | ".join(str(c) for c in row) + " |" for row in body]
    return "\n".join(lines)


def cross(rows, key_rows, row_of, col_of, cols, keep_empty=False):
    body = []
    for key, label in key_rows:
        sel = [r for r in rows if row_of(r) == key]
        if not sel and not keep_empty:
            continue
        counts = [sum(1 for r in sel if col_of(r) == c) for c in cols]
        body.append([label] + [n or "" for n in counts] + [len(sel)])
    totals = [sum(1 for r in rows if col_of(r) == c) for c in cols]
    body.append(["**Total**"] + [n or "" for n in totals] + [len(rows)])
    return body


def summary(rows):
    defects = [r for r in rows if r["kind"] == "defect"]
    claims = [r for r in rows if r["kind"] == "claim"]
    effects = [e for e in EFFECTS if any(r["effect"] == e for r in defects)]
    out = []
    out.append(f"Rows: {len(rows)}: {len(defects)} defects (D) and "
               f"{len(claims)} doc/claim errors (C).")
    shown = [r for r in defects if r["effect"] in VERDICT and r["shown"] == "yes"]
    out.append(f"Defects with a wrong verdict shown on a model: {len(shown)}.")
    out.append("")
    out.append("**Defects by discovery class and effect**")
    out.append("")
    out.append(table(["Discovery class"] + effects + ["Total"],
                     cross(defects, CLASSES, lambda r: r["class"],
                           lambda r: r["effect"], effects, keep_empty=True)))
    out.append("")
    out.append("**Wrong-verdict defects (wrong ok, wrong counterexample, wrong verdict) "
               "by discovery class and whether a run showed them**")
    out.append("")
    verdict = [r for r in defects if r["effect"] in VERDICT]
    shown_vals = ["yes", "no", "test-only", "unstated"]
    shown_vals = [s for s in shown_vals if any(r["shown"] == s for r in verdict)]
    out.append(table(["Discovery class"] + [f"shown: {s}" for s in shown_vals] + ["Total"],
                     cross(verdict, CLASSES, lambda r: r["class"],
                           lambda r: r["shown"], shown_vals)))
    out.append("")
    out.append("**Defects by component and effect**")
    out.append("")
    out.append(table(["Component"] + effects + ["Total"],
                     cross(defects, [(c, c) for c in COMPONENTS],
                           lambda r: r["component"], lambda r: r["effect"], effects)))
    out.append("")
    out.append("**Defects by component and discovery class**")
    out.append("")
    used = [k for k, _ in CLASSES if any(r["class"] == k for r in defects)]
    out.append(table(["Component"] + used + ["Total"],
                     cross(defects, [(c, c) for c in COMPONENTS],
                           lambda r: r["component"], lambda r: r["class"], used)))
    out.append("")
    out.append("**Subclasses (all rows)**")
    out.append("")
    body = []
    for key, label in CLASSES:
        subs = sorted({r["subclass"] for r in rows if r["class"] == key and r["subclass"]})
        for s in subs:
            d = sum(1 for r in defects if r["class"] == key and r["subclass"] == s)
            c = sum(1 for r in claims if r["class"] == key and r["subclass"] == s)
            body.append([label, s, d or "", c or ""])
    out.append(table(["Discovery class", "Subclass", "Defects", "Claims"], body, labels=2))
    out.append("")
    out.append("**Doc/claim errors by discovery class**")
    out.append("")
    out.append(table(["Discovery class", "Claims"],
                     [[label, sum(1 for r in claims if r["class"] == key)]
                      for key, label in CLASSES
                      if any(r["class"] == key for r in claims)]
                     + [["**Total**", len(claims)]]))
    return "\n".join(out)


def full(rows):
    header = ["ID", "Ref", "Version", "Date", "Component", "Effect", "Shown",
              "Class", "Description", "Evidence"]
    body = []
    for r in rows:
        cls = r["class"] + (f" ({r['subclass']})" if r["subclass"] else "")
        cells = [r["id"], r["ref"], r["version"], r["date"], r["component"],
                 r["effect"], r["shown"], cls, r["description"], r["evidence"]]
        body.append([c.replace("|", "\\|") for c in cells])
    lines = ["| " + " | ".join(header) + " |", "|" + "---|" * len(header)]
    lines += ["| " + " | ".join(row) + " |" for row in body]
    return "\n".join(lines)


def main(argv):
    args = [a for a in argv if a != "--table"]
    root = pathlib.Path(__file__).resolve().parents[2]
    path = pathlib.Path(args[0]) if args else root / "docs/paper/evidence/defects.tsv"
    rows = load(path)
    print(full(rows) if "--table" in argv else summary(rows))


if __name__ == "__main__":
    main(sys.argv[1:])
