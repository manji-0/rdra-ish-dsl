#!/usr/bin/env bash
# Read changed paths on stdin, one per line; print `false` when every one of them is prose,
# which no build compiles and only the prose checks read, else `true`. On `false` the
# `check` job skips its `test` and `sample artifact snapshots` steps and runs the prose
# checks instead: the tests that read Markdown (`stage_docs_are_in_sync`,
# `rule_ids_are_unique_and_in_the_docs`, `export_gate`) and the skill snippets.
#
#   git diff --name-only HEAD^1 HEAD | scripts/code-relevant.sh
#
# Anything not listed as prose counts as code, so a new kind of file runs every check until
# it is listed here. An empty list prints `true`.
set -euo pipefail

code=false
seen=false
while IFS= read -r path; do
  [[ -z "$path" ]] && continue
  seen=true
  case "$path" in
    # Markdown that scripts/check-sample-artifacts.sh regenerates and compares: a hand edit
    # must still fail there. Every file under samples/ is code for the same reason, and
    # because tests read sample READMEs (guards_cli reads samples/oidc-auth/README.md).
    docs/diagram-sample-review.md | samples/*) code=true ;;
    # Prose. A `case` `*` matches `/` too, so these cover the subdirectories.
    docs/*.md | docs/*.png | skills/*.md | .audit/* | .claude/*) ;;
    # Any other file in a subdirectory, such as an `.rdra` under skills/ that tests check.
    */*) code=true ;;
    # Prose at the top level: README.md, CHANGELOG.md, CLAUDE.md, LICENSE.
    *.md | LICENSE) ;;
    *) code=true ;;
  esac
done

if $code || ! $seen; then echo true; else echo false; fi
