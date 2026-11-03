#!/usr/bin/env python3
"""Rewrite .rdra sources to the 1.0 syntax: effects name `Entity.column`, rules their
entities, and `.role(...)` picks the key of a join.

usage: migrate-syntax-v3.py [--dry-run] PATH...

Each PATH is a directory (every .rdra file under it, `out/` skipped) or a single file
of any extension, so a Markdown page or a test source holding DSL text can be passed
by name; review that diff, since only the patterns below are touched.

Rewrites, each of which the parser now rejects with a message naming the new form:
  1. sets(UseCase, Entity, column == value)        ->  sets(UseCase, Entity.column == value)
     sets(UseCase, Entity, column == Src.column)   ->  sets(UseCase, Entity.column == Src.column)
  2. drives(UseCase, Entity, column op rhs, b)     ->  drives(UseCase, Entity.column op rhs, b)
  3. sets(UseCase, Entity, column op rhs, b)       ->  drives(UseCase, Entity.column op rhs, b)
  4. forbidden(A.x == 1, B.y == 2)                 ->  forbidden(A, B, A.x == 1, B.y == 2)
     (and `required`, `exclusive`): the entities the conditions name, in order, lead.
     invariant().when(A.x == 1).then(B.y == 2)     ->  invariant(A, B).when(...).then(...)
     The entities of every `.when(...)`, then of every `.then(...)`, lead. A rule over
     one entity becomes that entity's rule: `CrossForbidden_A_0` is now `Forbidden_A_0`,
     `CrossInv_A_0` is now `Inv_A_0`. A rule whose conditions qualify no column is left
     for the load error to name.
  5. .by(role) after holds, sets, forbidden, invariant  ->  .role(role)
     `.by(...)` that picks the key of a join is `.role(...)`, as in `relate(..).role(..)`.
     `belongs(Buc, Business).by(Medium)` names a medium and keeps `.by`.

A call and its chain may span lines. Text-level and
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
CALL_RE = re.compile(
    r"(?:(?<=\\n)|(?<![\w.]))(?P<name>sets|drives|forbidden|required|exclusive|invariant|holds)\("
)
RULES = ("forbidden", "required", "exclusive", "invariant")
KEY_SELECTING = ("holds", "sets", "forbidden", "invariant")
CHAIN_RE = re.compile(r"\s*(?://[^\n]*\s*)*\.(\w+)\(")
QCOL_RE = re.compile(r"(?<![\w.:])((?:\w+::)?[A-Za-z_]\w*)\.[A-Za-z_]\w*")
CMP_RE = re.compile(r"==|!=|<=|>=|<|>|\bin\b")
STRING_RE = re.compile(r'"(?:[^"\\]|\\.)*"')


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
class ChainCall:
    name: str
    args: list[str]
    name_at: int
    close: int


def chain_after(text: str, close: int) -> list[ChainCall]:
    """The `.name(args)` calls chained after the `)` at `close`."""
    calls = []
    at = close + 1
    while (m := CHAIN_RE.match(text, at)) is not None:
        parsed = call_args(text, m.end() - 1)
        if parsed is None:
            break
        args, end = parsed
        calls.append(ChainCall(m.group(1), args, m.start(1), end))
        at = end + 1
    return calls


def rename_by(text: str, close: int, chain: list[ChainCall]) -> tuple[str, int] | None:
    """The chain after the `)` at `close` with each `.by(` renamed `.role(`, and the index
    after it, or `None` when the chain has no `.by(...)`."""
    bys = [cc for cc in chain if cc.name == "by"]
    if not bys:
        return None
    out, at = [], close + 1
    for cc in bys:
        out += [text[at : cc.name_at], "role"]
        at = cc.name_at + len("by")
    end = chain[-1].close + 1
    out.append(text[at:end])
    return "".join(out), end


def entities(conditions: list[str]) -> list[str]:
    """The entities of the `Entity.column` operands of `conditions`, in order, once each."""
    found: list[str] = []
    for cond in conditions:
        for m in QCOL_RE.finditer(STRING_RE.sub('""', cond)):
            if m.group(1) not in found:
                found.append(m.group(1))
    return found


def rewrite_rule(name: str, args: list[str], chain: list[ChainCall]) -> str | None:
    """The argument text of a rule written without its leading entity, with the entities its
    conditions name in front, or `None` when the rule has its entity or names none."""
    if name == "invariant":
        if any(arg.strip() for arg in args):
            return None
        conditions = [a for method in ("when", "then") for cc in chain if cc.name == method for a in cc.args]
        scope = entities(conditions)
        return ", ".join(scope) if scope else None
    if not args or not CMP_RE.search(STRING_RE.sub('""', args[0])):
        return None
    scope = entities(args)
    if not scope:
        return None
    rest = ",".join(args)
    return ", ".join(scope) + "," + (rest if rest[:1].isspace() else " " + rest)


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
        name = m.group("name")
        chain = chain_after(text, close)
        kinds = []
        rest = ",".join(args)
        if name in RULES:
            if (scoped := rewrite_rule(name, args, chain)) is not None:
                rest = scoped
                kinds.append("rule-scope")
        elif name in ("sets", "drives"):
            if (rewritten := rewrite_call(name, args)) is not None:
                name, rest, kind = rewritten
                kinds.append(kind)
        end = close + 1
        tail = ""
        if m.group("name") in KEY_SELECTING and (renamed := rename_by(text, close, chain)):
            tail, end = renamed
            kinds.append("by-role")
        if not kinds:
            continue
        new = f"{name}({rest}){tail}"
        old = text[m.start() : end]
        line = text.count("\n", 0, m.start()) + 1
        print(f"{path}:{line}: - {' '.join(old.split())}")
        print(f"{path}:{line}: + {' '.join(new.split())}")
        report.edits.update(kinds)
        out.append(text[pos : m.start()])
        out.append(new)
        pos = end
    out.append(text[pos:])
    return "".join(out)


def migrate_file(path: Path, dry_run: bool) -> Report:
    report = Report()
    text = path.read_bytes().decode("utf-8")
    new_text = migrate_text(text, path, report)
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
    print("next: rdra-ish fmt --write <workspace> && rdra-ish check <workspace>")
    return 0


if __name__ == "__main__":
    sys.exit(main())
