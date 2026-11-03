#!/usr/bin/env python3
"""Rewrite .rdra sources to the 1.0 syntax: every column an effect names is `Entity.column`.

usage: migrate-syntax-v3.py [--dry-run] PATH...

Each PATH is a directory (every .rdra file under it, `out/` skipped) or a single file
of any extension, so a Markdown page or a test source holding DSL text can be passed
by name; review that diff, since only the patterns below are touched.

Rewrites, each of which the parser now rejects with a message naming the new form:
  1. sets(UseCase, Entity, column == value)        ->  sets(UseCase, Entity.column == value)
     sets(UseCase, Entity, column == Src.column)   ->  sets(UseCase, Entity.column == Src.column)
  2. drives(UseCase, Entity, column op rhs, b)     ->  drives(UseCase, Entity.column op rhs, b)
  3. sets(UseCase, Entity, column op rhs, b)       ->  drives(UseCase, Entity.column op rhs, b)

A call may span lines, and a chain after it (`.by(...)`) is kept as written. Text-level and
idempotent: the new forms match none of the old patterns, so a second run changes nothing.
Every edit is printed.
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path

ENTITY_RE = re.compile(r"^\s*(?:\w+::)?\w+(?:\.\w+)*\s*$")
LHS_RE = re.compile(r"^(?P<ws>\s*)(?P<col>[A-Za-z_]\w*)(?P<rest>\s*(?:==|!=|<=|>=|<|>|in\b|not\s+in\b).*)$", re.S)
TRUTH_RE = re.compile(r"^\s*(?:true|false)\s*$")
# A call starts a line, follows a non-word character, or follows the `\n` escape of a string
# literal in a test source.
CALL_RE = re.compile(r"(?:(?<=\\n)|(?<![\w.]))(?P<name>sets|drives)\(")


def call_args(text: str, open_at: int) -> tuple[list[str], int] | None:
    """The top-level arguments of the call whose `(` is at `open_at`, and the index of its `)`."""
    depth = 0
    args: list[str] = []
    start = open_at + 1
    i = open_at
    while i < len(text):
        c = text[i]
        if c == '"':
            j = i + 1
            while j < len(text) and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            i = j + 1
            continue
        if c in "([":
            depth += 1
        elif c in ")]":
            depth -= 1
            if depth == 0:
                args.append(text[start:i])
                return args, i
        elif c == "," and depth == 1:
            args.append(text[start:i])
            start = i + 1
        elif c == "\n" and text.startswith("\n\n", i):
            return None
        i += 1
    return None


def fold_entity(entity: str, cmp: str) -> str | None:
    """`cmp` with its bare left-hand column written `entity.column`, or `None` when `cmp` is no
    comparison; a column already written `Entity.column` is kept."""
    m = LHS_RE.match(cmp)
    if m is None:
        return None
    return f"{m.group('ws')}{entity.strip()}.{m.group('col')}{m.group('rest')}"


def rewrite_call(name: str, args: list[str]) -> tuple[str, str, str] | None:
    """The rewritten call (name and argument text) and the kind of edit, or `None`."""
    if len(args) not in (3, 4) or not ENTITY_RE.match(args[1]):
        return None
    if len(args) == 3 and name != "sets":
        return None
    if len(args) == 4 and not TRUTH_RE.match(args[3]):
        return None
    cmp = args[2]
    qualified = re.match(r"^\s*(?:\w+::)?\w+\.\w+", cmp) is not None
    folded = cmp if qualified else fold_entity(args[1], cmp)
    if folded is None:
        return None
    rest = ",".join([args[0], folded, *args[3:]])
    if name == "sets" and len(args) == 4:
        return "drives", rest, "sets-truth"
    return name, rest, name


@dataclass
class Report:
    edits: Counter = field(default_factory=Counter)
    files: list[Path] = field(default_factory=list)

    def add(self, other: "Report") -> None:
        self.edits.update(other.edits)
        self.files += other.files

    def summary(self) -> str:
        kinds = "; ".join(f"{k} {v}" for k, v in sorted(self.edits.items()))
        return f"{sum(self.edits.values())} edits in {len(self.files)} files; {kinds}"


def migrate_text(text: str, path: Path, report: Report) -> str:
    out: list[str] = []
    pos = 0
    for m in CALL_RE.finditer(text):
        if m.start() < pos:
            continue
        parsed = call_args(text, m.end() - 1)
        if parsed is None:
            continue
        args, close = parsed
        rewritten = rewrite_call(m.group("name"), args)
        if rewritten is None:
            continue
        name, rest, kind = rewritten
        new = f"{name}({rest})"
        old = text[m.start() : close + 1]
        line = text.count("\n", 0, m.start()) + 1
        print(f"{path}:{line}: - {' '.join(old.split())}")
        print(f"{path}:{line}: + {' '.join(new.split())}")
        report.edits[kind] += 1
        out.append(text[pos : m.start()])
        out.append(new)
        pos = close + 1
    out.append(text[pos:])
    return "".join(out)


def migrate_file(path: Path, dry_run: bool) -> Report:
    report = Report()
    text = path.read_text(encoding="utf-8")
    new_text = migrate_text(text, path, report)
    if new_text != text:
        report.files.append(path)
        if not dry_run:
            path.write_text(new_text, encoding="utf-8")
    return report


def files_under(root: Path) -> list[Path]:
    if root.is_file():
        return [root]
    return sorted(
        p for p in root.rglob("*.rdra") if "out" not in p.relative_to(root).parts[:-1]
    )


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("paths", nargs="+", type=Path)
    ap.add_argument("--dry-run", action="store_true", help="print the edits without writing")
    args = ap.parse_args()

    total = Report()
    for root in args.paths:
        if not root.exists():
            print(f"error: {root} does not exist", file=sys.stderr)
            return 2
        for path in files_under(root):
            total.add(migrate_file(path, args.dry_run))
    print(f"TOTAL: {total.summary()}")
    print("next: rdra-ish fmt --write <workspace> && rdra-ish check <workspace>")
    return 0


if __name__ == "__main__":
    sys.exit(main())
