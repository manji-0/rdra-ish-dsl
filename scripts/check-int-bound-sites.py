#!/usr/bin/env python3
"""Check that z3.md's Lemma U still names every place the code reads the Int bound.

Usage: python3 -I scripts/check-int-bound-sites.py [--list] [repo root]

Lemma U of docs/theory/z3.md argues that the system the build returns depends on the Int
bound `m` (`Bounds::int_max`) only in five listed ways, from a reading of every place the
bound is read. This script keeps that reading's list of places in step with the code. It
fails, naming the site, when a site reads the bound that the list does not hold, or when a
listed site no longer reads it; the author then reads the site, updates Lemma U's proof
sketch, and updates the list. `--list` prints the sites the code has, in the list's format.

What counts as a site. A site is a pair (token, `path::item`):

- token `int_max`: the identifier `int_max` in any `crates/*/src/**/*.rs`, read or written
  (a field declaration or a struct literal counts too);
- token `of_number`: the identifier `of_number` there too (`Domain::of_number` turns a
  literal into a value under the bound);
- token `size`: a method call `.size(` in `crates/rdra-ish-core/src/lifecycle/build/`, where
  it is `Domain::size`, the number of values of a domain under the bound.

`path` is relative to the repository root. `item` is the chain of named items that encloses
the token, outermost first: `mod` (inline modules only), `impl` (its self type, generics
dropped), `trait`, `fn`, `struct`, `enum`, `union`, `const`, `static`, joined by `::`, or
`<file>` at the top level. Lines and formatting do not matter, so moving code inside an item,
reformatting it, or reading the bound a second time in a listed item passes.

Left out: comments, string and character literals, files in a `tests` directory or named
`tests.rs`, items under `#[cfg(test)]` or `#[cfg(kani)]` (or `#[cfg(all(test, ...))]`), and the files
of a module declared as `#[cfg(test)] mod name;`.

What it does not see: a read through a helper with another name (only `Domain::size` and
`Domain::of_number` are followed, and `size` only inside `build/`), a derived read such as a
second pass over a list the bound filled (as when `arm_guards` reads a
`Choose` list), and slot counts, which Lemma U reads too. It holds the list, not the lemma.

The list lives in z3.md in a fenced block whose info string is `int-bound-sites`: one site
per line, `token path::item`, in any order, with `#` starting a comment.
"""

import os
import re
import sys

DOC = "docs/theory/z3.md"
FENCE = "int-bound-sites"
BUILD = "crates/rdra-ish-core/src/lifecycle/build/"

ITEM = re.compile(r"\b(mod|trait|fn|struct|enum|union|const|static)\s+(?:mut\s+)?(?!fn\b)([A-Za-z_]\w*)")
IMPL = re.compile(r"\bimpl\b(.*)$", re.S)
TEST_CFG = re.compile(r"#\[\s*cfg\s*\(\s*(?:all\s*\(\s*)?(?:test|kani)\b")
TEST_MOD = re.compile(
    TEST_CFG.pattern
    + r"[^\]]*\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+([A-Za-z_]\w*)\s*;"
)
RAW = re.compile(r'b?r(#*)"')
CHAR = re.compile(r"'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^\\'\n])'")


def strip(src):
    """The source with comments removed and string and char literals emptied."""
    out = []
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        word_before = i > 0 and (src[i - 1].isalnum() or src[i - 1] == "_")
        if src.startswith("//", i):
            j = src.find("\n", i)
            i = n if j < 0 else j
        elif src.startswith("/*", i):
            depth, i = 1, i + 2
            while i < n and depth:
                if src.startswith("/*", i):
                    depth, i = depth + 1, i + 2
                elif src.startswith("*/", i):
                    depth, i = depth - 1, i + 2
                else:
                    i += 1
        elif not word_before and (m := RAW.match(src, i)):
            end = src.find('"' + m.group(1), m.end())
            out.append('""')
            i = n if end < 0 else end + 1 + len(m.group(1))
        elif c == '"' or (c == "b" and not word_before and src.startswith('b"', i)):
            i += 2 if c == "b" else 1
            while i < n and src[i] != '"':
                i += 2 if src[i] == "\\" else 1
            out.append('""')
            i += 1
        elif c == "'" and (m := CHAR.match(src, i)):
            out.append("' '")
            i = m.end()
        else:
            out.append(c)
            i += 1
    return "".join(out)


def impl_name(header):
    """The self type of an `impl` header: the last path segment, generics dropped."""
    text = IMPL.search(header).group(1)
    flat, depth = [], 0
    for ch in text:
        if ch == "<":
            depth += 1
        elif ch == ">":
            depth = max(depth - 1, 0)
        elif depth == 0:
            flat.append(ch)
    flat = re.split(r"\bwhere\b", "".join(flat))[0]
    if re.search(r"\bfor\b", flat):
        flat = re.split(r"\bfor\b", flat)[-1]
    names = re.findall(r"[A-Za-z_]\w*", flat)
    return names[-1] if names else "impl"


def frame(header):
    """(name, is_test) for the item a `{` opens; name is None for a plain block."""
    test = bool(TEST_CFG.search(header))
    if re.search(r"\bimpl\b", header) and not re.search(r"\bfn\b", header):
        return impl_name(header), test
    found = ITEM.findall(header)
    if found:
        return found[-1][1], test
    # The braces of a struct literal in `const C: T = T { .. }` belong to `C`.
    const = re.search(r"\b(?:const|static)\s+(?:mut\s+)?([A-Za-z_]\w*)\s*:", header)
    return (const.group(1) if const else None), test


def sites(path, rel):
    """Every (token, path::item) in one file."""
    code = strip(open(path, encoding="utf-8").read())
    tokens = [("int_max", re.compile(r"\bint_max\b")), ("of_number", re.compile(r"\bof_number\b"))]
    if rel.startswith(BUILD):
        tokens.append(("size", re.compile(r"\.\s*size\s*\(")))
    marks = {}
    for token, rx in tokens:
        for m in rx.finditer(code):
            marks.setdefault(m.start(), []).append((token, m.end()))
    found = set()
    stack = []  # (name or None, is_test), one per open brace
    start = 0  # where the current header starts: after the last `{`, `}` or `;`
    for i, ch in enumerate(code):
        for token, end in marks.get(i, ()):
            if any(test for _, test in stack) or TEST_CFG.search(code[start:i]):
                continue
            names = [name for name, _ in stack if name]
            # A token in an item's own header: `fn of_number(..)`, `fn f(int_max: u8)`,
            # `const C: u8 = B.int_max;`.
            item = ITEM.findall(code[start:end])
            if item and item[-1][0] in ("fn", "const", "static"):
                names.append(item[-1][1])
            found.add((token, f"{rel}::{'::'.join(names) or '<file>'}"))
        if ch == "{":
            stack.append(frame(code[start:i]))
            start = i + 1
        elif ch == "}":
            if stack:
                stack.pop()
            start = i + 1
        elif ch == ";":
            start = i + 1
    return found


def test_modules(path):
    """The file and directory of each module `path` declares as `#[cfg(test)] mod name;`."""
    code = strip(open(path, encoding="utf-8").read())
    stem = os.path.splitext(os.path.basename(path))[0]
    here = os.path.dirname(path)
    if stem not in ("mod", "lib", "main"):
        here = os.path.join(here, stem)
    out = set()
    for name in TEST_MOD.findall(code):
        out |= {os.path.join(here, name + ".rs"), os.path.join(here, name)}
    return out


def scan(root):
    files = []
    crates = os.path.join(root, "crates")
    for crate in sorted(os.listdir(crates)):
        src = os.path.join(crates, crate, "src")
        for dirpath, dirnames, filenames in os.walk(src):
            dirnames[:] = sorted(d for d in dirnames if d != "tests")
            files += [
                os.path.join(dirpath, f)
                for f in sorted(filenames)
                if f.endswith(".rs") and f != "tests.rs"
            ]
    gated = set()
    for path in files:
        gated |= test_modules(path)
    out = set()
    for path in files:
        if path in gated or any(path.startswith(g + os.sep) for g in gated):
            continue
        out |= sites(path, os.path.relpath(path, root).replace(os.sep, "/"))
    return out


def listed(root):
    """The sites the list in z3.md names, and those it names twice."""
    lines = open(os.path.join(root, DOC), encoding="utf-8").read().splitlines()
    out, dup, inside, seen = set(), [], False, False
    for line in lines:
        s = line.strip()
        if not inside and re.match(r"^(`{3,}|~{3,})\s*" + re.escape(FENCE) + r"\s*$", s):
            inside, seen = True, True
            continue
        if inside and re.match(r"^(`{3,}|~{3,})\s*$", s):
            inside = False
            continue
        if not inside:
            continue
        s = s.split("#", 1)[0].strip()
        if not s:
            continue
        parts = s.split()
        if len(parts) != 2:
            sys.exit(f"{DOC}: cannot read {line!r} in the {FENCE} block: want `token path::item`")
        site = (parts[0], parts[1])
        if site in out:
            dup.append(site)
        out.add(site)
    if not seen:
        sys.exit(f"{DOC}: no {FENCE} block (it belongs under Lemma U)")
    return out, dup


def main():
    args = sys.argv[1:]
    show = "--list" in args
    args = [a for a in args if a != "--list"]
    root = args[0] if args else os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    have = scan(root)
    if show:
        for token, site in sorted(have, key=lambda s: (s[1], s[0])):
            print(f"{token} {site}")
        return 0
    want, dup = listed(root)
    new, gone = sorted(have - want), sorted(want - have)
    for token, site in dup:
        print(f"{DOC}: `{token} {site}` is listed twice in the {FENCE} block")
    for token, site in new:
        print(f"new site: {site} reads the Int bound through `{token}`, and the {FENCE} "
              f"list under Lemma U in {DOC} does not name it")
    for token, site in gone:
        print(f"removed site: {site} no longer reads the Int bound through `{token}` (or it "
              f"was renamed or moved), but the {FENCE} list under Lemma U in {DOC} names it")
    if new or gone or dup:
        print(f"\nRead each site above and decide whether it makes the built system depend on "
              f"the Int bound in a way Lemma U's items 1 to 5 do not cover. Update Lemma U's "
              f"proof sketch in {DOC} (and Definition 3 if the lemma changes), then the "
              f"{FENCE} list.")
        return 1
    print(f"{len(have)} Int-bound sites, as the {FENCE} list under Lemma U in {DOC} names them")
    return 0


if __name__ == "__main__":
    sys.exit(main())
