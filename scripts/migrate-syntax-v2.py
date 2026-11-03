#!/usr/bin/env python3
"""Rewrite .rdra sources to the v2 syntax: use case first, `Entity.column`, one spelling.

usage: migrate-syntax-v2.py [--dry-run] PATH...

Each PATH is a directory (every .rdra file under it, `out/` skipped) or a single file
of any extension, so a Markdown page or a test source holding DSL text can be passed
by name; review that diff, since only the patterns below are touched.

Rewrites, each of which the parser now rejects with a message naming the new form:
  1. transitions(Entity.column, UseCase, a -> b)  ->  transitions(UseCase, Entity.column, a -> b)
  2. maps_field(Field, Entity, "column"), maps_field(Field, Entity, column) and
     maps_field(Field, Entity, Entity.column)       ->  maps_field(Field, Entity.column)
  3. acceptance criteria "..." (and acceptance "...")  ->  acceptance_criteria "..."
  4. writes(X, Entity)                              ->  updates(X, Entity)
     `writes` was an update under another name. A use case that only creates or only
     deletes the entity should say `creates` or `deletes`; the script cannot tell.

Text-level and idempotent: the new forms match none of the old patterns, so a second
run changes nothing. Comments and string contents are left alone. Every edit is printed.
"""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

QREF = r"(?:\w+::)?\w+"
TRANSITIONS_RE = re.compile(
    r"\btransitions\(\s*(?P<col>\w+(?:\.\w+)+)\s*,\s*(?P<uc>" + QREF + r")\s*,(?P<ws>\s*)"
)
MAPS_FIELD_RE = re.compile(
    r"\bmaps_field\(\s*(?P<field>" + QREF + r"(?:\.\w+)*)\s*,\s*(?P<entity>" + QREF
    + r"(?:\.\w+)*?)\s*,\s*(?P<col>\"[^\"]*\"|\w+(?:\.\w+)*)\s*\)"
)
ACCEPTANCE_RE = re.compile(r"\bacceptance(?:[ \t]+criteria)?(?=[ \t]+\")")
WRITES_RE = re.compile(r"^(?P<indent>\s*)writes\(")
STRING_RE = re.compile(r'"(?:[^"\\]|\\.)*"')


def split_comment(line: str) -> tuple[str, str]:
    """Split a line into code and a trailing `//` comment (outside strings)."""
    in_str = False
    i = 0
    while i < len(line):
        c = line[i]
        if c == "\\" and in_str:
            i += 2
            continue
        if c == '"':
            in_str = not in_str
        elif not in_str and line.startswith("//", i):
            return line[:i], line[i:]
        i += 1
    return line, ""


def outside_strings(code: str, pattern: re.Pattern[str], repl) -> tuple[str, int]:
    """Apply `pattern` -> `repl` to the matches of `code` that start outside string literals."""
    strings = [m.span() for m in STRING_RE.finditer(code)]
    count = 0

    def sub(m: re.Match[str]) -> str:
        nonlocal count
        if any(start <= m.start() < end for start, end in strings):
            return m.group(0)
        count += 1
        return repl(m) if callable(repl) else m.expand(repl)

    return pattern.sub(sub, code), count


def swap_transitions(m: re.Match[str]) -> str:
    return f"transitions({m.group('uc')}, {m.group('col')},{m.group('ws')}"


def merge_maps_field(m: re.Match[str]) -> str:
    col = m.group("col")
    col = col[1:-1] if col.startswith('"') else col.split(".")[-1]
    return f"maps_field({m.group('field')}, {m.group('entity')}.{col})"


@dataclass
class Report:
    transitions: int = 0
    maps_field: int = 0
    acceptance: int = 0
    writes: int = 0
    files: list[Path] = field(default_factory=list)

    @property
    def edits(self) -> int:
        return self.transitions + self.maps_field + self.acceptance + self.writes

    def add(self, other: "Report") -> None:
        self.transitions += other.transitions
        self.maps_field += other.maps_field
        self.acceptance += other.acceptance
        self.writes += other.writes
        self.files += other.files

    def summary(self) -> str:
        return (
            f"{self.edits} edits in {len(self.files)} files; transitions {self.transitions}; "
            f"maps_field {self.maps_field}; acceptance_criteria {self.acceptance}; "
            f"writes {self.writes}"
        )


def migrate_line(line: str, report: Report) -> str:
    body = line.rstrip("\r\n")
    end = line[len(body):]
    code, comment = split_comment(body)
    code, n = outside_strings(code, TRANSITIONS_RE, swap_transitions)
    report.transitions += n
    code, n = outside_strings(code, MAPS_FIELD_RE, merge_maps_field)
    report.maps_field += n
    code, n = outside_strings(code, ACCEPTANCE_RE, "acceptance_criteria")
    report.acceptance += n
    code, n = WRITES_RE.subn(r"\g<indent>updates(", code)
    report.writes += n
    return code + comment + end


def migrate_file(path: Path, dry_run: bool) -> Report:
    report = Report()
    text = path.read_bytes().decode("utf-8")
    lines = text.splitlines(keepends=True)
    out = []
    for i, line in enumerate(lines):
        before = report.edits
        new = migrate_line(line, report)
        if report.edits != before:
            print(f"{path}:{i + 1}: - {line.strip()}")
            print(f"{path}:{i + 1}: + {new.strip()}")
        out.append(new)
    new_text = "".join(out)
    if new_text != text:
        report.files.append(path)
        if not dry_run:
            path.write_bytes(new_text.encode("utf-8"))
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
    if total.writes:
        print("note: `writes` became `updates`; say `creates` or `deletes` where that is what it meant")
    print("next: rdra-ish fmt --check <workspace> && rdra-ish check <workspace>")
    return 0


if __name__ == "__main__":
    sys.exit(main())
