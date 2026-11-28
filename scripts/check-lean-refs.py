#!/usr/bin/env python3
"""Check that the names the Lean proofs' docs cite still name a Rust or Lean definition.

Usage: python3 -I scripts/check-lean-refs.py [--sides] [repo root]

proofs/lean/README.md pairs each Lean theorem with a claim and says which Rust code the
proofs do and do not read. docs/theory/claims.md and docs/theory/cones.md name Lean
theorems beside the code they argue about, and the Lean sources' doc comments name Rust
code too. This script fails, naming file:line, when one of those names no longer resolves,
so a rename on either side cannot leave the correspondence pointing at nothing.

What it checks, by place and form of the backticked text:

- proofs/lean/README.md, a table whose first header is `Lean (`RdraIsh.`)`: each backticked
  name in the first column is a Lean declaration `RdraIsh.<name>` in the file of the `File`
  column, and `CheckAxioms.lean` holds `#print axioms RdraIsh.<name>`. A name without a
  namespace that is not declared at the top level takes the namespace of the name before it
  in the cell (`ExampleRise.bisim`, `c_after_flag` is `RdraIsh.ExampleRise.c_after_flag`).
- `File.lean::name` anywhere in the checked text: the same, in that file, where `name` may
  leave out a namespace the file opens (`Split.lean::no_counter_input` is
  `RdraIsh.Act.no_counter_input`). A path such as `proofs/lean/RdraIsh/Split.lean` alone
  must exist.
- A Rust path: `path.rs::a::b`, `Type::item` or `module::item` (identifiers joined by `::`,
  with or without a file), and `::item` after a `path.rs::...` on the same line, which reads
  `item` in that file. The last segment is a definition (`fn`, `struct`, `enum`, `trait`,
  `type`, `const`, `static`, `mod`, a struct field or an enum variant) in `crates/`, and
  the other segments name, in order, modules or items that enclose it. A trailing argument
  list is dropped (`Reduction::stages(before, after)`). A file path is matched by its
  trailing segments; a leading `core/`, `cli/`, `emit/`, `lsp/` or `syntax/` is the crate
  `rdra-ish-<that>`, and directories may be left out in between (`core/reduce/split.rs` is
  `crates/rdra-ish-core/src/lifecycle/reduce/split.rs`).
- proofs/lean/README.md only: a bare snake_case name with an underscore (`split_at`) names a
  Rust definition, a Lean declaration, field or constructor, or a keyword of the language
  (`leads_to`, a `Token::Ident("…")` of crates/rdra-ish-syntax/src); a dotted name whose
  head is a Lean namespace or type of the project (`InnerMap.path_fwd`, `Stable.b2`) names
  a Lean declaration. `--sides` prints which side each bare name resolves on, so a name the prose
  means as code that resolves only through Lean shows up; such a name is qualified in the
  README (`Reduction::stages`) rather than listed here.

Checked text: all of proofs/lean/README.md; the doc comments (`/-- -/`, `/-! -/`) of
proofs/lean/**/*.lean, for Rust paths and `File.lean::name`; and docs/theory/claims.md and
docs/theory/cones.md, for Rust paths and `File.lean::name` on each line that names a Lean
file (`.lean`).

What it does not see: bare names outside the README (in claims.md and cones.md they cannot
be told from corpus cases and test names), single words (`Extended`, `stages`, `obs`),
names Lean has but the project does not declare (`List.mem_cons`), a Rust item behind a
macro, and whether the prose says something true of the name it cites.
"""

import importlib.util
import os
import re
import sys

README = "proofs/lean/README.md"
LEAN = "proofs/lean"
AXIOMS = "proofs/lean/CheckAxioms.lean"
THEORY = ("docs/theory/claims.md", "docs/theory/cones.md")
CRATE_ALIAS = {"core", "cli", "emit", "lsp", "syntax"}

sys.dont_write_bytecode = True  # no scripts/__pycache__ from the import below
HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location(
    "int_bound_sites", os.path.join(HERE, "check-int-bound-sites.py")
)
_ibs = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_ibs)
frame, ITEM = _ibs.frame, _ibs.ITEM

IDENT = r"[A-Za-z_]\w*"
CHAIN = rf"{IDENT}(?:::{IDENT})*"
RUST_PATH = re.compile(
    rf"^(?:(?P<file>[\w./-]+\.rs)(?:::(?P<a>{CHAIN}))?|(?P<b>{IDENT}(?:::{IDENT})+)|::(?P<c>{CHAIN}))$"
)
LEAN_REF = re.compile(rf"^(?P<file>[\w./-]+\.lean)(?:::(?P<name>{IDENT}(?:\.{IDENT})*))?$")
SNAKE = re.compile(r"^[a-z][a-z0-9]*(?:_[a-z0-9]+)+$")
DOTTED = re.compile(rf"^({IDENT})\.({IDENT}(?:\.{IDENT})*)$")
TICK = re.compile(r"`([^`\n]+)`")
# Comments, literals and lifetimes (`'static` is no `static` item), emptied before the brace
# walk. A regex pass, not check-int-bound-sites.py's character loop: the index reads every
# file under crates/, tests included, and must stay well under 2 s.
NOISE = re.compile(
    r"//[^\n]*|/\*(?:[^*]|\*(?!/))*\*/"  # comments (block comments do not nest here)
    r"|b?r(#*)\".*?\"\1|b?\"(?:\\.|[^\"\\])*\""  # strings, raw strings
    r"|'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^\\'\n])'"  # chars
    r"|'[A-Za-z_]\w*",  # lifetimes
    re.S,
)
BRACE = re.compile(r"[{};]")
UNBRACED = re.compile(rf"\b(fn|struct|enum|union|const|static|type|mod|trait)\s+(?:mut\s+)?({IDENT})")
MACRO = re.compile(rf"\bmacro_rules!\s*({IDENT})")


def blank(m):
    return re.sub(r"[^\n]", " ", m.group(0))


def module_path(rel):
    """The module segments of a file under a crate: `src/lifecycle/reduce/mod.rs` is
    `lifecycle::reduce`; a file under `tests/` is its stem."""
    parts = rel.split("/")[2:]  # drop crates/<crate>
    if parts[0] in ("src", "tests", "examples"):
        parts = parts[1:]
    parts[-1] = parts[-1][:-3]
    if parts[-1] in ("mod", "lib", "main"):
        parts = parts[:-1]
    return parts


def body_members(body, kind):
    """Field names of a struct body, variant names of an enum body."""
    flat, depth = [], 0
    for ch in body:
        if ch in "{([<":
            depth += 1
        elif ch in "})]>":
            depth = max(depth - 1, 0)
        elif depth == 0:
            flat.append(ch)
    text = re.sub(r"#\s*", "", "".join(flat))
    if kind == "struct":
        return re.findall(rf"(?:^|,)\s*(?:pub\b\s*)?({IDENT})\s*:", text)
    return re.findall(rf"(?:^|,)\s*({IDENT})", text)


def rust_defs(path, rel):
    """Every definition in one file, as the tuple of its full path (module segments, then
    the enclosing items, then its name)."""
    code = NOISE.sub(blank, open(path, encoding="utf-8").read())
    mod = tuple(module_path(rel))
    out = set()
    stack = []  # (name or None, kind, body start)
    start = 0
    for m in BRACE.finditer(code):
        i, ch = m.start(), m.group(0)
        header = code[start:i]
        names = tuple(n for n, _, _ in stack if n)
        if ch == "{":
            name, _ = frame(header)
            found = ITEM.findall(header)
            kind = found[-1][0] if found and not re.search(r"\bimpl\b", header) else "impl"
            if name:
                out.add(mod + names + (name,))
            for mac in MACRO.findall(header):
                out.add(mod + names + (mac,))
            stack.append((name, kind, i + 1))
        elif ch == "}":
            if stack:
                name, kind, body = stack.pop()
                if name and kind in ("struct", "enum", "union"):
                    inner = tuple(n for n, _, _ in stack if n) + (name,)
                    for member in body_members(code[body:i], kind):
                        out.add(mod + inner + (member,))
        else:
            # `struct A;`, `struct A(u8);`, `const C: u8 = 3;`, `type T = U;`, `fn f();`,
            # `mod name;`, and tuple-struct fields are not read.
            for kind, name in UNBRACED.findall(header):
                out.add(mod + names + (name,))
        start = i + 1
    return out


def rust_index(root):
    """rel path -> set of definition paths."""
    index = {}
    crates = os.path.join(root, "crates")
    for dirpath, dirnames, filenames in os.walk(crates):
        dirnames[:] = sorted(d for d in dirnames if d != "target")
        for f in sorted(filenames):
            if f.endswith(".rs"):
                path = os.path.join(dirpath, f)
                rel = os.path.relpath(path, root).replace(os.sep, "/")
                index[rel] = rust_defs(path, rel)
    return index


def subsequence(want, have):
    """`want` ends where `have` ends, and its other segments occur in `have` in order."""
    if not want or not have or want[-1] != have[-1]:
        return False
    it = iter(have[:-1])
    return all(any(seg == h for h in it) for seg in want[:-1])


def files_for(index, spec):
    """The indexed files a (possibly shortened) path names."""
    segs = spec.strip("/").split("/")
    crate = None
    if len(segs) > 1 and segs[0] in CRATE_ALIAS:
        crate, segs = f"crates/rdra-ish-{segs[0]}/", segs[1:]
    out = []
    for rel in index:
        if crate and not rel.startswith(crate):
            continue
        parts = rel.split("/")
        if parts[-len(segs):] == segs:
            out.append(rel)
            continue
        # Directories left out in between: the last segment matches and the rest occur in
        # order (`core/tests/reduce.rs` for `.../src/lifecycle/tests/reduce.rs`).
        if parts[-1] == segs[-1] and subsequence(tuple(segs), tuple(parts)):
            out.append(rel)
    return out


def tree(index, rel):
    """A file and the files of the modules it declares (`x.rs` and `x/`, `mod.rs` and its
    directory)."""
    stem = rel[:-3]
    if os.path.basename(stem) in ("mod", "lib", "main"):
        stem = os.path.dirname(stem)
    return [r for r in index if r == rel or r.startswith(stem + "/")]


class Lean:
    def __init__(self, root):
        self.decls = {}  # full name -> file (relative to proofs/lean)
        self.members = set()  # fields and constructors, full names
        self.heads = set()  # namespaces and types, last segment
        self.docs = []  # (rel path, line, text) of doc comments
        base = os.path.join(root, LEAN)
        for dirpath, dirnames, filenames in os.walk(base):
            dirnames[:] = sorted(d for d in dirnames if not d.startswith("."))
            for f in sorted(filenames):
                if f.endswith(".lean"):
                    path = os.path.join(dirpath, f)
                    self.read(path, os.path.relpath(path, root).replace(os.sep, "/"))
        self.axioms = set(
            re.findall(r"^#print axioms (\S+)", open(os.path.join(root, AXIOMS), encoding="utf-8").read(), re.M)
        )
        self.names = set(self.decls) | self.members

    def read(self, path, rel):
        text = open(path, encoding="utf-8").read()
        for m in re.finditer(r"/-[-!](.*?)-/", text, re.S):
            self.docs.append((rel, text.count("\n", 0, m.start()) + 1, m.group(1)))
        text = re.sub(r"/-.*?-/", blank, text, flags=re.S)
        text = re.sub(r"--[^\n]*", "", text)
        scopes = []  # (kind, segments)
        owner = None  # full name of the structure or inductive whose members follow
        for line in text.splitlines():
            if m := re.match(r"^\s*namespace\s+(\S+)", line):
                scopes.append(("namespace", m.group(1).split(".")))
                self.heads.update(m.group(1).split("."))
                owner = None
                continue
            if m := re.match(r"^\s*section\b\s*(\S*)", line):
                scopes.append(("section", []))
                owner = None
                continue
            if re.match(r"^\s*end\b", line):
                if scopes:
                    scopes.pop()
                owner = None
                continue
            ns = [s for _, segs in scopes for s in segs]
            m = re.match(
                r"^(?:@\[[^\]]*\]\s*)?(?:(?:private|protected|noncomputable|partial|unsafe)\s+)*"
                r"(theorem|lemma|def|abbrev|structure|inductive|class|instance|opaque)\s+([^\s({:\[]+)",
                line,
            )
            if m:
                full = ".".join(ns + m.group(2).split("."))
                self.decls[full] = rel
                owner = full if m.group(1) in ("structure", "inductive", "class") else None
                if owner:
                    self.heads.add(full.split(".")[-1])
                    for ctor in re.findall(rf"\|\s*({IDENT})", line):
                        self.members.add(f"{owner}.{ctor}")
                continue
            if owner and line[:1] in (" ", "\t", "|"):
                if m := re.match(rf"^\s*\|\s*({IDENT})", line):
                    self.members.add(f"{owner}.{m.group(1)}")
                elif m := re.match(rf"^\s{{2}}({IDENT})\s*(?:[({{\[][^:]*)?:", line):
                    self.members.add(f"{owner}.{m.group(1)}")
            elif line.strip():
                owner = None

    def resolve(self, name, file=None, prefix=None):
        """The full name `name` cites (in `file`, relative to proofs/lean/, when given), or
        None. An elided namespace is filled from `prefix`, then from any namespace of the
        file."""
        cands = [f"RdraIsh.{name}"]
        if prefix:
            cands.append(f"RdraIsh.{prefix}.{name}")
        for full in cands:
            if full in self.decls and (file is None or self.decls[full].endswith("/" + file)):
                return full
        if file:
            hits = [f for f, r in self.decls.items() if r.endswith("/" + file) and f.endswith("." + name)]
            if len(hits) == 1:
                return hits[0]
        return None


def lean_table_rows(lines):
    """(line number, first cell, file cell) of each row of a table headed `Lean (`RdraIsh.`)`."""
    inside = False
    for n, line in enumerate(lines, 1):
        if line.startswith("| Lean (`RdraIsh.`)"):
            inside = True
            continue
        if inside and not line.startswith("|"):
            inside = False
        if inside and not re.match(r"^\|[-| ]+\|$", line):
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            yield n, cells[0], cells[1] if len(cells) > 1 else ""


class Checker:
    def __init__(self, root):
        self.root = root
        self.rust = rust_index(root)
        self.lean = Lean(root)
        self.last = {}  # rust definition last segment -> True
        for defs in self.rust.values():
            for d in defs:
                self.last[d[-1]] = True
        self.keywords = set()
        syntax = os.path.join(root, "crates", "rdra-ish-syntax", "src")
        for dirpath, _, filenames in os.walk(syntax):
            for f in filenames:
                if f.endswith(".rs"):
                    text = open(os.path.join(dirpath, f), encoding="utf-8").read()
                    self.keywords.update(re.findall(r'Token::Ident\("(\w+)"', text))
        self.errors = []
        self.sides = []
        self.count = 0

    def fail(self, where, msg):
        self.errors.append(f"{where}: {msg}")

    def rust_resolves(self, file, chain):
        if file:
            rels = files_for(self.rust, file)
            if not rels:
                return f"no file under crates/ matches `{file}`"
            if not chain:
                return None
            want = tuple(chain)
            for rel in rels:
                for r in tree(self.rust, rel):
                    if any(subsequence(want, d) for d in self.rust[r]):
                        return None
            return f"no definition `{'::'.join(chain)}` in `{file}`"
        want = tuple(chain)
        if want[-1] in self.last:
            for defs in self.rust.values():
                if any(subsequence(want, d) for d in defs):
                    return None
        return f"no Rust definition `{'::'.join(chain)}` under crates/"

    def check_lean_ref(self, where, file, name, table=False):
        self.count += 1
        base = os.path.basename(file)
        if not name:
            if not os.path.exists(os.path.join(self.root, file)) and not any(
                r.endswith("/" + base) for r in self.lean.decls.values()
            ):
                self.fail(where, f"`{file}`: no such Lean file")
            return
        if not any(r.endswith("/" + base) for r in self.lean.decls.values()):
            self.fail(where, f"`{file}::{name}`: no such Lean file")
            return
        full = self.lean.resolve(name, base)
        if not full:
            self.fail(where, f"`{file}::{name}`: no Lean declaration `{name}` in {base}")
        elif full not in self.lean.axioms:
            self.fail(where, f"`{full}` is not listed in {AXIOMS} (`#print axioms {full}`)")

    def check_token(self, where, tok, ctx, readme):
        """One backticked token; ctx holds the last Rust file named on the line."""
        tok = tok.strip()
        tok = re.sub(r"\(.*\)$", "", tok)  # a call: `Reduction::stages(before, after)`
        if m := LEAN_REF.match(tok):
            self.check_lean_ref(where, m.group("file"), m.group("name"))
            return
        if m := RUST_PATH.match(tok):
            if m.group("file"):
                ctx["file"] = m.group("file")
                chain = m.group("a").split("::") if m.group("a") else []
                file = m.group("file")
            elif m.group("c"):
                if not ctx.get("file"):
                    return  # `::name` with no file before it on the line: not a path
                file, chain = ctx["file"], m.group("c").split("::")
            else:
                file, chain = None, m.group("b").split("::")
            self.count += 1
            if err := self.rust_resolves(file, chain):
                self.fail(where, err)
            return
        if not readme:
            return
        if SNAKE.match(tok):
            self.count += 1
            rust = tok in self.last
            lean = any(n == f"RdraIsh.{tok}" or n.endswith("." + tok) for n in self.lean.names)
            side = "both" if rust and lean else "rust" if rust else "lean" if lean else None
            if side is None and tok in self.keywords:
                side = "keyword"
            self.sides.append((where, tok, side or "none"))
            if side is None:
                self.fail(where, f"`{tok}`: no Rust definition under crates/, Lean declaration or keyword")
            return
        if (m := DOTTED.match(tok)) and m.group(1) in self.lean.heads:
            self.count += 1
            if not any(n == f"RdraIsh.{tok}" or n.endswith("." + tok) for n in self.lean.names):
                self.fail(where, f"`{tok}`: no Lean declaration")

    def check_readme(self):
        lines = open(os.path.join(self.root, README), encoding="utf-8").read().splitlines()
        table = {}
        for n, first, file in lean_table_rows(lines):
            table[n] = (first, file)
        fence = False
        for n, line in enumerate(lines, 1):
            if line.lstrip().startswith("```"):
                fence = not fence
                continue
            if fence:
                continue
            where = f"{README}:{n}"
            ctx = {}
            if n in table:
                first, file = table[n]
                lean_file = (TICK.findall(file) or [""])[0]
                prefix = None
                for name in TICK.findall(first):
                    self.count += 1
                    full = self.lean.resolve(name, lean_file or None, prefix)
                    if not full:
                        self.fail(where, f"`{name}`: no Lean declaration `RdraIsh.{name}` in {lean_file or 'proofs/lean'}")
                    elif full not in self.lean.axioms:
                        self.fail(where, f"`{full}` is not listed in {AXIOMS} (`#print axioms {full}`)")
                    if "." in name:
                        prefix = name.rsplit(".", 1)[0]
                rest = line.split("|", 3)[3] if line.count("|") >= 3 else ""
                for tok in TICK.findall(rest):
                    if not tok.endswith(".lean"):
                        self.check_token(where, tok, ctx, True)
                continue
            for tok in TICK.findall(line):
                self.check_token(where, tok, ctx, True)

    def check_lean_docs(self):
        for rel, line0, text in self.lean.docs:
            for k, line in enumerate(text.splitlines()):
                ctx = {}
                for tok in TICK.findall(line):
                    self.check_token(f"{rel}:{line0 + k}", tok, ctx, False)

    def check_theory(self):
        for doc in THEORY:
            fence = False
            for n, line in enumerate(open(os.path.join(self.root, doc), encoding="utf-8"), 1):
                if line.lstrip().startswith("```"):
                    fence = not fence
                    continue
                if fence or ".lean" not in line:
                    continue
                ctx = {}
                for tok in TICK.findall(line):
                    self.check_token(f"{doc}:{n}", tok, ctx, False)


def main():
    args = sys.argv[1:]
    sides = "--sides" in args
    args = [a for a in args if a != "--sides"]
    root = args[0] if args else os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    c = Checker(root)
    c.check_readme()
    c.check_lean_docs()
    c.check_theory()
    if sides:
        for where, tok, side in c.sides:
            print(f"{side:7} {tok}  {where}")
    for e in c.errors:
        print(e)
    if c.errors:
        print(
            f"\n{len(c.errors)} name(s) above resolve to nothing. Point the prose at the "
            f"definition's present name (or path), or, for a Lean theorem the README tables "
            f"list, add `#print axioms` to {AXIOMS}."
        )
        return 1
    print(f"check-lean-refs: {c.count} names in the Lean proofs' docs resolve")
    return 0


if __name__ == "__main__":
    sys.exit(main())
