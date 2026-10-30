#!/usr/bin/env python3
"""Copy committed sample artifacts into the Markdown documents that embed them.

A fenced block preceded by a line `<!-- embed: <path> -->` is rewritten to hold that file,
the path relative to the document's directory. Every `rdra-ish` command a document prints in a
`sh` block, other than `check`, must be a row of scripts/sample-artifacts.tsv that writes the
path its `--out` names, so the printed commands are the ones scripts/check-sample-artifacts.sh
runs.

Usage: python3 -I scripts/embed-sample-artifacts.py
(scripts/check-sample-artifacts.sh runs it after regenerating the artifacts.)
"""
import re
import shlex
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DOCS = sorted(ROOT.glob('samples/incremental-order/step-*/design.md')) + [
    ROOT / 'docs/diagram-sample-review.md'
]
MARKER = re.compile(r'^<!-- embed: (\S+) -->$')
EXTENSION = {'diagram': '.mmd'}


def manifest():
    rows = {}
    for line in (ROOT / 'scripts/sample-artifacts.tsv').read_text().splitlines():
        if line and not line.startswith('#'):
            out, cmd = line.split('\t')
            rows[out] = cmd
    return rows


def embed(doc, lines):
    out, i = [], 0
    while i < len(lines):
        line = lines[i]
        out.append(line)
        i += 1
        match = MARKER.match(line)
        if not match:
            continue
        source = doc.parent / match.group(1)
        if i >= len(lines) or not lines[i].startswith('```'):
            sys.exit(f'{doc}: `{line}` is not followed by a fenced block')
        out.append(lines[i])
        end = lines.index('```', i + 1)
        out.extend(l.rstrip() for l in source.read_text().rstrip('\n').split('\n'))
        out.append('```')
        i = end + 1
    return out


def sh_commands(lines):
    """The `rdra-ish` commands of the `sh` blocks, each continuation line joined."""
    commands, in_sh, current = [], False, ''
    for line in lines:
        if line.startswith('```'):
            in_sh = line == '```sh'
            continue
        if not in_sh:
            continue
        current += line.strip()
        if current.endswith('\\'):
            current = current[:-1]
            continue
        if current.startswith('rdra-ish '):
            commands.append(current)
        current = ''
    return commands


def check_commands(doc, lines, rows):
    errors = []
    for line in sh_commands(lines):
        args = shlex.split(line)[1:]
        if args[0] == 'check':
            continue
        if '--out' not in args:
            errors.append(f'{doc}: no --out in `{line}`')
            continue
        at = args.index('--out')
        target = args[at + 1] + EXTENSION.get(args[0], '')
        command = ' '.join(args[:at] + args[at + 2:])
        if rows.get(target) != command:
            errors.append(f'{doc}: `{line}` is not the sample-artifacts.tsv row for {target}')
    return errors


def main():
    rows = manifest()
    errors = []
    for doc in DOCS:
        lines = doc.read_text().split('\n')
        errors += check_commands(doc.relative_to(ROOT), lines, rows)
        doc.write_text('\n'.join(embed(doc, lines)))
    if errors:
        sys.exit('\n'.join(errors))


main()
