#!/usr/bin/env python3
"""Migrate .rdra sources to the phase 3 surface (use-case-driven transitions, timers, external systems).

usage: migrate-phase3.py [--keep EVENT[,EVENT...]] [--dry-run] WORKSPACE...

Each WORKSPACE is a directory (every .rdra file under it, `out/` skipped) or a
single .rdra file. References resolve within one workspace only, so pass each
independent model as its own argument.

Steps:
  1. `timing X "..."` becomes `timer X "..."`, and `.when(...)` is deleted from
     `belongs(...)`. A timer that should start a use case needs a
     `triggers(Timer, UseCase)` line the codemod cannot guess.
  2. transitions(E.col, Ev, a -> b) becomes one transitions(E.col, UC, a -> b)
     per raises(UC, Ev). The line stays in place when the raiser's `raises` line
     is in the same file, otherwise it is written next to that `raises` line.
  3. sets(event::Ev, E, ...) fans out to sets(UC, E, ...) the same way.
  4. `uses(Actor, ExtSystem)` is deleted. Connect the external system through events:
     `raises(ExtSystem, Event)` inbound, `triggers(Event, ExtSystem)` outbound.
  5. An event with no remaining reference besides its declaration and its
     `raises` lines (no triggers, outbox, covers, unmigrated transitions or sets)
     is deleted together with its `raises` lines. Events named by --keep stay.

Text-level and idempotent: a second run changes nothing. Every edit and every
case it cannot decide is printed; it never guesses a use case.
"""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

DECL_KINDS = {
    "actor", "extsystem", "system", "requirement", "adr", "nfr", "quality",
    "constraint", "business", "buc", "flow", "step", "usecase", "screen",
    "field", "event", "entity", "api", "dto", "timing", "timer", "medium", "permission",
    "variation", "property",
}
DECL_RE = re.compile(r"^\s*([a-z_]+)\s+([A-Za-z_]\w*)\b")
RAISES_RE = re.compile(r"^\s*raises\(\s*((?:usecase::)?\w+)\s*,\s*((?:event::)?\w+)\s*\)\s*$")
TIMING_RE = re.compile(r"^(\s*)timing(\s+\w+\b.*)$")
BELONGS_RE = re.compile(r"^\s*belongs\(")
CHAIN_WHEN_RE = re.compile(r"^\s*\.when\(")
USES_RE = re.compile(r"^\s*uses\(\s*(?:\w+::)?(\w+)\s*,\s*(?:\w+::)?(\w+)\s*\)\s*$")
TRANSITIONS_RE = re.compile(r"^(\s*)transitions\(\s*([\w.]+)\s*,\s*((?:\w+::)?\w+)\s*,\s*(.*)\)\s*$")
SETS_RE = re.compile(r"^(\s*)sets\(\s*((?:\w+::)?\w+)\s*,\s*(.*)\)\s*$")
TOKEN_RE = re.compile(r"(?:\b(\w+)::)?\b([A-Za-z_]\w*)\b")
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


@dataclass
class SourceFile:
    path: Path
    lines: list[str]
    # line index -> replacement lines (empty list deletes the line)
    replace: dict[int, list[str]] = field(default_factory=dict)
    # line index -> lines inserted after it
    insert_after: dict[int, list[str]] = field(default_factory=dict)

    def code(self, i: int) -> str:
        return split_comment(self.lines[i].rstrip("\n"))[0]

    def apply(self) -> list[str]:
        """Apply the edits; blank lines a deletion would leave doubled or at end of file are dropped."""
        out: list[str] = []
        deleted = False
        tail_deleted = False  # a deletion since the last non-blank line
        for i, line in enumerate(self.lines):
            new = self.replace.get(i, [line]) + self.insert_after.get(i, [])
            if not new:
                deleted = tail_deleted = True
                continue
            for n in new:
                if n.strip() == "" and deleted and out and out[-1].strip() == "":
                    continue
                out.append(n)
                deleted = False
                if n.strip():
                    tail_deleted = False
        if tail_deleted:
            while out and out[-1].strip() == "":
                out.pop()
        return out


@dataclass
class Report:
    edits: int = 0
    timings_renamed: int = 0
    whens_deleted: int = 0
    uses_deleted: int = 0
    transitions_migrated: int = 0
    transitions_written: int = 0
    sets_migrated: int = 0
    sets_written: int = 0
    events_deleted: list[str] = field(default_factory=list)
    raises_deleted: int = 0
    kept: list[str] = field(default_factory=list)
    undecidable: list[str] = field(default_factory=list)


def load_workspace(root: Path) -> list[SourceFile]:
    if root.is_file():
        paths = [root]
    else:
        paths = sorted(
            p for p in root.rglob("*.rdra")
            if "out" not in p.relative_to(root).parts[:-1]
        )
    return [SourceFile(p, p.read_text(encoding="utf-8").splitlines(keepends=True)) for p in paths]


def declarations(files: list[SourceFile]) -> dict[str, set[str]]:
    """id -> set of kinds declaring it."""
    decls: dict[str, set[str]] = {}
    for f in files:
        for i in range(len(f.lines)):
            m = DECL_RE.match(f.code(i))
            if m and m.group(1) in DECL_KINDS:
                decls.setdefault(m.group(2), set()).add(m.group(1))
    return decls


def event_name(arg: str, decls: dict[str, set[str]]) -> tuple[str | None, bool]:
    """Return (event id, ambiguous) for a predicate argument naming an event."""
    if arg.startswith("event::"):
        return arg[len("event::"):], False
    if "::" in arg:
        return None, False
    kinds = decls.get(arg, set())
    if "event" not in kinds:
        return None, False
    if kinds - {"event"}:
        return arg, True
    return arg, False


def raisers(files: list[SourceFile], decls) -> dict[str, list[tuple[SourceFile, int, str]]]:
    """event id -> [(file, line index, use case argument as written)]."""
    out: dict[str, list[tuple[SourceFile, int, str]]] = {}
    for f in files:
        for i in range(len(f.lines)):
            m = RAISES_RE.match(f.code(i))
            if not m:
                continue
            ev, _ = event_name(m.group(2), decls)
            if ev is not None:
                out.setdefault(ev, []).append((f, i, m.group(1)))
    return out


def where(f: SourceFile, i: int) -> str:
    return f"{f.path}:{i + 1}"


def strip_inline_when(code: str) -> str:
    """Remove every `.when(...)` call (string-aware) from one `belongs(...)` line."""
    out = []
    i = 0
    while i < len(code):
        if code.startswith(".when(", i):
            depth, j, in_str = 0, i + len(".when"), False
            while j < len(code):
                c = code[j]
                if c == "\\" and in_str:
                    j += 2
                    continue
                if c == '"':
                    in_str = not in_str
                elif not in_str and c == "(":
                    depth += 1
                elif not in_str and c == ")":
                    depth -= 1
                    if depth == 0:
                        break
                j += 1
            i = j + 1
            continue
        out.append(code[i])
        i += 1
    return "".join(out)


def surface_changes(files, report: Report) -> None:
    """Step 1 (`timing` to `timer`, `belongs(...).when` removal) and step 4 (`uses` removal)."""
    for f in files:
        in_belongs = False
        for i in range(len(f.lines)):
            code = f.code(i)
            line = f.lines[i].rstrip("\n")
            comment = split_comment(line)[1]
            m = TIMING_RE.match(code)
            if m:
                new = f"{m.group(1)}timer{m.group(2)}" + (f"  {comment}" if comment else "")
                f.replace[i] = [new + "\n"]
                print(f"{where(f, i)}: - {line.strip()}")
                print(f"{where(f, i)}: + {new.strip()}")
                report.timings_renamed += 1
                report.edits += 1
                continue
            if BELONGS_RE.match(code):
                in_belongs = True
                if ".when(" in code:
                    stripped = strip_inline_when(code).rstrip()
                    f.replace[i] = [stripped + (f"  {comment}" if comment else "") + "\n"]
                    print(f"{where(f, i)}: - {line.strip()}")
                    print(f"{where(f, i)}: + {stripped.strip()}")
                    report.whens_deleted += 1
                    report.edits += 1
                continue
            if in_belongs and code.strip().startswith("."):
                if CHAIN_WHEN_RE.match(code):
                    f.replace[i] = []
                    print(f"{where(f, i)}: - {line.strip()}")
                    report.whens_deleted += 1
                    report.edits += 1
                continue
            in_belongs = False
            u = USES_RE.match(code)
            if u:
                f.replace[i] = []
                print(f"{where(f, i)}: - {line.strip()}")
                print(
                    f"{where(f, i)}: extsystem {u.group(2)} lost its only link; "
                    f"add raises({u.group(2)}, Event) or triggers(Event, {u.group(2)})"
                )
                report.uses_deleted += 1
                report.edits += 1


def fan_out(files, decls, report: Report) -> None:
    by_event = raisers(files, decls)
    for f in files:
        for i in range(len(f.lines)):
            if i in f.replace:
                continue
            code = f.code(i)
            line = f.lines[i].rstrip("\n")
            comment = split_comment(line)[1]
            m = TRANSITIONS_RE.match(code)
            if m:
                indent, col, arg, rest = m.groups()
                make = lambda uc, indent=indent, col=col, rest=rest: f"{indent}transitions({col}, {uc}, {rest})"
                kind = "transitions"
            else:
                m = SETS_RE.match(code)
                if not m:
                    continue
                indent, arg, rest = m.groups()
                make = lambda uc, indent=indent, rest=rest: f"{indent}sets({uc}, {rest})"
                kind = "sets"
            ev, ambiguous = event_name(arg, decls)
            if ev is None:
                continue
            if ambiguous:
                report.undecidable.append(
                    f"{where(f, i)}: {kind} names `{arg}`, which is both an event and another kind; qualify it and rerun"
                )
                continue
            ucs = by_event.get(ev, [])
            if not ucs:
                report.undecidable.append(
                    f"{where(f, i)}: cannot migrate: no use case raises {ev}; add a use case that drives `{line.strip()}`"
                )
                continue
            suffix = f"  {comment}" if comment else ""
            in_place: list[str] = []
            for rf, ri, uc in ucs:
                new = make(uc)
                if rf is f:
                    in_place.append(new + suffix + "\n")
                    print(f"{where(f, i)}: + {new}")
                else:
                    rf.insert_after.setdefault(ri, []).append(new + "\n")
                    print(f"{where(rf, ri)}: + {new}   (moved from {where(f, i)})")
                report.edits += 1
            f.replace[i] = in_place
            print(f"{where(f, i)}: - {line.strip()}")
            report.edits += 1
            if kind == "transitions":
                report.transitions_migrated += 1
                report.transitions_written += len(ucs)
            else:
                report.sets_migrated += 1
                report.sets_written += len(ucs)


def references(files: list[SourceFile], decls) -> dict[str, int]:
    """event id -> count of references outside its declaration and raises lines."""
    counts: dict[str, int] = {}
    events = {k for k, kinds in decls.items() if "event" in kinds}
    for f in files:
        for raw in f.apply():
            code = split_comment(raw.rstrip("\n"))[0]
            if not code.strip() or RAISES_RE.match(code):
                continue
            if re.match(r"^\s*(module|import)\b", code):
                continue
            decl = DECL_RE.match(code)
            stripped = STRING_RE.sub('""', code)
            for qual, name in TOKEN_RE.findall(stripped):
                if name not in events:
                    continue
                if decl and decl.group(1) in DECL_KINDS and decl.group(2) == name and not qual:
                    continue
                if qual == "event" or (not qual and decls[name] == {"event"}):
                    counts[name] = counts.get(name, 0) + 1
    return counts


def delete_unconsumed(files, decls, keep: set[str], report: Report) -> None:
    # Re-read the post-fan-out text so step 5 sees step 2/3 results.
    staged = [SourceFile(f.path, f.apply()) for f in files]
    refs = references(staged, decls)
    by_event = raisers(staged, decls)
    for f in staged:
        for i in range(len(f.lines)):
            m = DECL_RE.match(f.code(i))
            if not m or m.group(1) != "event":
                continue
            ev = m.group(2)
            if refs.get(ev, 0):
                continue
            if ev in keep:
                report.kept.append(ev)
                print(f"{where(f, i)}: kept (--keep): event {ev} has no consumer")
                continue
            if "{" in f.code(i):
                report.undecidable.append(f"{where(f, i)}: event {ev} has a block body; delete it by hand")
                continue
            f.replace[i] = []
            print(f"{where(f, i)}: - {f.lines[i].strip()}   (event {ev} has no consumer)")
            report.events_deleted.append(ev)
            report.edits += 1
            # Indented clause lines (`  description "..."`) belong to the declaration.
            j = i + 1
            while j < len(f.lines) and f.lines[j][:1] in (" ", "\t") and f.code(j).strip():
                f.replace[j] = []
                print(f"{where(f, j)}: - {f.lines[j].strip()}")
                report.edits += 1
                j += 1
            for rf, ri, _ in by_event.get(ev, []):
                rf.replace[ri] = []
                print(f"{where(rf, ri)}: - {rf.lines[ri].strip()}")
                report.raises_deleted += 1
                report.edits += 1
    for orig, new in zip(files, staged):
        orig.lines = new.apply()
        orig.replace.clear()
        orig.insert_after.clear()


def migrate(root: Path, keep: set[str], dry_run: bool) -> Report:
    report = Report()
    files = load_workspace(root)
    decls = declarations(files)
    surface_changes(files, report)
    fan_out(files, decls, report)
    delete_unconsumed(files, decls, keep, report)
    if not dry_run:
        for f in files:
            text = "".join(f.lines)
            if text != f.path.read_text(encoding="utf-8"):
                f.path.write_text(text, encoding="utf-8")
    return report


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("workspaces", nargs="+", type=Path)
    ap.add_argument("--keep", action="append", default=[],
                    help="event ids to keep even without a consumer (comma-separated, repeatable)")
    ap.add_argument("--dry-run", action="store_true", help="print the edits without writing")
    args = ap.parse_args()
    keep = {e.strip() for k in args.keep for e in k.split(",") if e.strip()}

    total = Report()
    for root in args.workspaces:
        print(f"== {root}")
        r = migrate(root, keep, args.dry_run)
        for u in r.undecidable:
            print(f"UNDECIDED {u}")
        print(
            f"summary {root}: {r.edits} edits; timings {r.timings_renamed}; belongs .when {r.whens_deleted}; "
            f"uses {r.uses_deleted}; transitions {r.transitions_migrated} -> {r.transitions_written}; "
            f"sets {r.sets_migrated} -> {r.sets_written}; events deleted {len(r.events_deleted)} "
            f"(raises deleted {r.raises_deleted}); kept {len(r.kept)}; undecided {len(r.undecidable)}"
        )
        total.edits += r.edits
        total.timings_renamed += r.timings_renamed
        total.whens_deleted += r.whens_deleted
        total.uses_deleted += r.uses_deleted
        total.transitions_migrated += r.transitions_migrated
        total.transitions_written += r.transitions_written
        total.sets_migrated += r.sets_migrated
        total.sets_written += r.sets_written
        total.events_deleted += r.events_deleted
        total.raises_deleted += r.raises_deleted
        total.kept += r.kept
        total.undecidable += r.undecidable
    print(
        f"TOTAL: {total.edits} edits; timings {total.timings_renamed}; belongs .when {total.whens_deleted}; "
        f"uses {total.uses_deleted}; transitions {total.transitions_migrated} -> {total.transitions_written}; "
        f"sets {total.sets_migrated} -> {total.sets_written}; events deleted {len(total.events_deleted)} "
        f"(raises deleted {total.raises_deleted}); kept {len(total.kept)} {sorted(total.kept)}; "
        f"undecided {len(total.undecidable)}"
    )
    print("next: rdra-ish check <workspace> && rdra-ish lint <workspace>")
    return 0


if __name__ == "__main__":
    sys.exit(main())
