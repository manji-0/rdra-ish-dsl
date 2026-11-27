#!/usr/bin/env bash
# Read changed paths on stdin, one per line; print `true` when one of them can change a
# verification result (what formal-verify and engine-equivalence check), else `false`.
#
#   git diff --name-only HEAD^1 HEAD | scripts/verify-relevant.sh
#
# The list below names what cannot: prose, the editor and LSP, the Claude Code plugin, the code generators, the
# diagram and data-layer exports, and the tests and fixtures of those. Anything not listed
# counts as relevant, so a new file runs the checks until it is listed here. An empty
# list prints `true`.
set -euo pipefail

relevant=false
seen=false
while IFS= read -r path; do
  [[ -z "$path" ]] && continue
  seen=true
  case "$path" in
    *.md | LICENSE | .gitignore | .gitattributes | .claude/* | docs/* | skills/* | editors/*) ;;
    crates/rdra-ish-lsp/*) ;;
    crates/rdra-ish-emit/src/codegen/* | crates/rdra-ish-emit/src/mermaid/*) ;;
    crates/rdra-ish-emit/src/asyncapi.rs | crates/rdra-ish-emit/src/csv.rs) ;;
    crates/rdra-ish-emit/src/dbml.rs | crates/rdra-ish-emit/src/json_schema.rs) ;;
    crates/rdra-ish-emit/src/markdown.rs | crates/rdra-ish-emit/src/mermaid.rs) ;;
    crates/rdra-ish-emit/src/openapi.rs) ;;
    crates/rdra-ish-cli/tests/codegen_* | crates/rdra-ish-cli/tests/diagram_*) ;;
    crates/rdra-ish-cli/tests/fixtures/codegen/* | crates/rdra-ish-cli/tests/next_snapshots/*) ;;
    crates/rdra-ish-cli/tests/snapshots/codegen_*) ;;
    .github/workflows/release.yml | .github/workflows/publish-pypi.yml) ;;
    .github/workflows/portability.yml | scripts/check-skill-snippets.py | pyproject.toml) ;;
    scripts/check-release-version.sh) ;;
    .claude-plugin/* | plugins/*) ;;
    crates/rdra-ish-cli/tests/lsp_cli.rs | crates/rdra-ish-cli/tests/plugin_manifest.rs) ;;
    *) relevant=true ;;
  esac
done

if $relevant || ! $seen; then echo true; else echo false; fi
