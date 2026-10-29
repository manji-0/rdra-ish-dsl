#!/usr/bin/env python3
"""Check the Agent Skills under skills/ against the rdra-ish binary.

Usage: python3 -I scripts/check-skill-snippets.py <rdra-ish binary> [skills dir]

Every Markdown file under the skills dir is read, and five things are checked:

- DSL blocks. A ```rdra fence is written to a temp file and run through `rdra-ish check`.
  Every `error` line fails the block, except the three a fragment cannot avoid:
  `undefined symbol`, `unknown module`, and `module file not found`. `check` stops at
  the first error, so a fragment (a block with one of those) is parsed and type-checked
  only, while a self-contained block also gets its lifecycle findings checked. The
  fences of one file that declare a `module`, and the fences that import one of those
  modules, are checked together as one model.
- Expected failures. A `<!-- expect-error -->` or `<!-- expect-error: TEXT -->` line right
  before a fence inverts it: the block must print an error (one containing TEXT).
- Untagged DSL. A bare ``` fence whose first line reads as DSL fails: tag it ```rdra.
- Transcripts. A fence whose first line is `$ rdra-ish ...` is run in the skill's
  directory, and the lines after it must appear in its output (stdout and stderr as one
  stream) in order and contiguously. A `...` line stands for any number of lines. Source sites print
  as absolute paths, so the skill directory's prefix is cut from them before comparing.
  A `verify` transcript needs `tlc` or `tlc2` on PATH.
- Command references. In code spans and fenced lines, every `rdra-ish <command>` must be
  a subcommand, every `--flag` one its `--help` lists, and every value of a flag with
  listed possible values one of them. A code span that starts with a subcommand name and
  holds a flag is checked the same way. Every `references/*.md`, `samples/*.rdra`, or
  `../` path in a code span must exist, relative to the skill (`../` to the file).

Prints each failure, then one summary line; exits 1 when anything failed.
"""

import glob
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile

SUBCOMMANDS = ["check", "lint", "files", "fmt", "diagram", "csv", "export", "verify", "list", "states"]
TOLERATED = re.compile(r"undefined symbol|unknown module|module file not found")
DSL_FIRST_LINE = re.compile(r'^(module |import |[a-z_]+ [A-Z]\w* "|[a-z_]+\()')
EXPECT_ERROR = re.compile(r"^<!--\s*expect-error(?::\s*(.*?))?\s*-->$")
FENCE = re.compile(r"^\s*```(\S*)\s*$")


class Help:
    def __init__(self, binary):
        self.binary = binary
        self.cache = {}

    def options(self, command):
        """Map each flag of `command` to its possible values (None when free-form)."""
        if command not in self.cache:
            out = subprocess.run([self.binary, command, "--help"], capture_output=True, text=True)
            self.cache[command] = parse_options(out.stdout) if out.returncode == 0 else None
        return self.cache[command]


def parse_options(text):
    options = {}
    current = None
    for line in text.splitlines():
        m = re.match(r"^\s+(?:-\w, )?(--[a-z][a-z-]*)", line)
        if m:
            current = m.group(1)
            options[current] = None
        if current is None:
            continue
        listed = re.search(r"\[possible values: ([^\]]+)\]", line)
        if listed:
            options[current] = {v.strip() for v in listed.group(1).split(",")}
        item = re.match(r"^\s+- ([a-z][a-z0-9-]*)(:|$)", line)
        if item:
            options[current] = (options[current] or set()) | {item.group(1)}
    return options


def fences(lines):
    """Yield (start line index, info string, body lines, expect-error marker or None)."""
    i = 0
    while i < len(lines):
        m = FENCE.match(lines[i])
        if not m:
            i += 1
            continue
        start, info = i, m.group(1)
        j = i + 1
        while j < len(lines) and not re.match(r"^\s*```\s*$", lines[j]):
            j += 1
        marker = EXPECT_ERROR.match(lines[start - 1].strip()) if start > 0 else None
        expect = None if marker is None else (marker.group(1) or "")
        yield start, info, lines[start + 1 : j], expect
        i = j + 1


def error_lines(output):
    return [l for l in output.splitlines() if l.startswith("error")]


class Checker:
    def __init__(self, binary, root):
        self.binary = os.path.abspath(binary)
        self.root = root
        self.help = Help(self.binary)
        self.failures = []
        self.counts = {"rdra": 0, "fragments": 0, "expect-error": 0, "transcripts": 0, "command refs": 0}

    def fail(self, where, message):
        self.failures.append(f"{where}: {message}")

    def run(self):
        for path in sorted(glob.glob(os.path.join(self.root, "**", "*.md"), recursive=True)):
            self.check_file(path)
        for failure in self.failures:
            print(failure)
        c = self.counts
        print(
            f"skill snippets: {c['rdra']} rdra blocks ({c['fragments']} fragments, "
            f"{c['expect-error']} expected errors), {c['transcripts']} transcripts, "
            f"{c['command refs']} command refs; {len(self.failures)} failures"
        )
        return 1 if self.failures else 0

    def skill_dir(self, path):
        rel = os.path.relpath(path, self.root).split(os.sep)
        return os.path.join(self.root, rel[0]) if len(rel) > 1 else self.root

    def check_file(self, path):
        lines = open(path, encoding="utf-8").read().splitlines()
        blocks = []
        in_fence = set()
        for start, info, body, expect in fences(lines):
            in_fence.update(range(start, start + len(body) + 2))
            where = f"{path}:{start + 1}"
            first = next((l.strip() for l in body if l.strip()), "")
            if first.startswith("$ rdra-ish "):
                self.check_transcript(path, where, first[2:], body[body.index(next(l for l in body if l.strip())) + 1 :])
            elif info == "rdra":
                blocks.append((where, body, expect))
            elif info == "" and DSL_FIRST_LINE.match(first):
                self.fail(where, "untagged fence holds DSL; tag it ```rdra")
            for n, line in enumerate(body, start + 2):
                self.check_commands(path, f"{path}:{n}", line, whole_line=True)
        for n, line in enumerate(lines, 1):
            if n - 1 in in_fence:
                continue
            for span in re.findall(r"`([^`\n]+)`", line):
                self.check_commands(path, f"{path}:{n}", span, whole_line=False)
                self.check_path(path, f"{path}:{n}", span)
        self.check_blocks(blocks)

    def check_blocks(self, blocks):
        declared = {}
        for i, (_, body, _) in enumerate(blocks):
            for line in body:
                m = re.match(r"^module ([\w.]+)", line.strip())
                if m:
                    declared[m.group(1)] = i
        grouped = set(declared.values())
        for i, (_, body, _) in enumerate(blocks):
            imports = {m.group(1) for l in body for m in [re.match(r"^import ([\w]+(?:\.[\w]+)*)", l.strip())] if m}
            if any(any(imp == mod or imp.startswith(mod + ".") for mod in declared) for imp in imports):
                grouped.add(i)
        with tempfile.TemporaryDirectory() as tmp:
            tmp = os.path.realpath(tmp)
            group_dir = os.path.join(tmp, "group")
            files = {}
            for i, (_, body, _) in enumerate(blocks):
                module = next((m.group(1) for l in body for m in [re.match(r"^module ([\w.]+)", l.strip())] if m), None)
                if i in grouped:
                    rel = module.replace(".", os.sep) + ".rdra" if module else f"snippet_{i}.rdra"
                    target = os.path.join(group_dir, rel)
                else:
                    target = os.path.join(tmp, f"snippet_{i}", "snippet.rdra")
                os.makedirs(os.path.dirname(target), exist_ok=True)
                with open(target, "w", encoding="utf-8") as f:
                    f.write("\n".join(body) + "\n")
                files[i] = target
            group_errors = []
            if grouped:
                out = subprocess.run([self.binary, "check", group_dir], capture_output=True, text=True)
                group_errors = error_lines(out.stdout + out.stderr)
            for i, (where, body, expect) in enumerate(blocks):
                self.counts["rdra"] += 1
                if i in grouped:
                    errors = [e for e in group_errors if files[i] in e or not any(f in e for f in files.values())]
                else:
                    out = subprocess.run([self.binary, "check", files[i]], capture_output=True, text=True)
                    errors = error_lines(out.stdout + out.stderr)
                errors = [e.replace(files[i], "<block>") for e in errors]
                hard = [e for e in errors if not TOLERATED.search(e)]
                if len(hard) < len(errors):
                    self.counts["fragments"] += 1
                if expect is not None:
                    self.counts["expect-error"] += 1
                    if not any(expect in e for e in hard):
                        wanted = f"an error containing {expect!r}" if expect else "an error"
                        self.fail(where, f"expected {wanted}, got {hard or 'none'}")
                elif hard:
                    for e in hard:
                        self.fail(where, e)

    def check_transcript(self, path, where, command, expected):
        self.counts["transcripts"] += 1
        argv = shlex.split(command)
        if len(argv) > 1 and argv[1] == "verify" and not (shutil.which("tlc") or shutil.which("tlc2")):
            self.fail(where, "verify transcript needs tlc or tlc2 on PATH")
            return
        cwd = os.path.realpath(self.skill_dir(path))
        out = subprocess.run(
            [self.binary] + argv[1:], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, cwd=cwd
        )
        actual = [l.replace(cwd + os.sep, "").rstrip() for l in out.stdout.splitlines()]
        segments, current = [], []
        for line in expected:
            if line.strip() == "...":
                segments.append(current)
                current = []
            else:
                current.append(line.rstrip())
        segments.append(current)
        pos = 0
        for segment in segments:
            if not segment:
                continue
            found = next(
                (k for k in range(pos, len(actual) - len(segment) + 1) if actual[k : k + len(segment)] == segment),
                None,
            )
            if found is None:
                self.fail(where, f"`{command}` no longer prints {segment[0]!r} (in order); actual output:\n" + "\n".join(actual))
                return
            pos = found + len(segment)

    def check_commands(self, path, where, text, whole_line):
        refs = [(m.group(1), m.group(2)) for m in re.finditer(r"rdra-ish ([a-z][a-z-]*)((?: [^`|]*)?)", text)]
        if not refs and not whole_line:
            m = re.match(r"^([a-z]+)((?: [^`|]*)?)$", text)
            if m and m.group(1) in SUBCOMMANDS and "--" in m.group(2):
                refs = [(m.group(1), m.group(2))]
        for command, rest in refs:
            self.counts["command refs"] += 1
            options = self.help.options(command)
            if options is None:
                self.fail(where, f"unknown subcommand `rdra-ish {command}`")
                continue
            words = rest.split()
            for k, word in enumerate(words):
                flag = re.match(r"^(--[a-z][a-z-]*)", word)
                if not flag:
                    continue
                if flag.group(1) not in options:
                    self.fail(where, f"`{command}` has no flag {flag.group(1)}")
                    continue
                values = options[flag.group(1)]
                value = word.split("=", 1)[1] if "=" in word else (words[k + 1] if k + 1 < len(words) else "")
                value = value.strip("`'\",.;:)")
                if values and value and not value.startswith(("<", "$", "-")) and value not in values:
                    self.fail(where, f"`{command} {flag.group(1)}` has no value {value!r}")

    def check_path(self, path, where, span):
        m = re.match(r"^((?:\.\./)+[\w./-]+|references/[\w./-]+\.md|samples/[\w./-]+\.rdra)(?:#.*)?$", span)
        if not m:
            return
        target = m.group(1)
        base = os.path.dirname(path) if target.startswith("../") else self.skill_dir(path)
        if not os.path.exists(os.path.join(base, target)):
            self.fail(where, f"`{target}` does not exist")


def main():
    if len(sys.argv) not in (2, 3):
        print(__doc__.strip().splitlines()[2], file=sys.stderr)
        return 2
    root = sys.argv[2] if len(sys.argv) == 3 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "skills")
    return Checker(sys.argv[1], os.path.normpath(root)).run()


if __name__ == "__main__":
    sys.exit(main())
