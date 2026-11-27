#!/usr/bin/env bash
# Copy the Agent Skills into the Claude Code plugin:
#
#   scripts/sync-plugin-skills.sh
#
# skills/ is the one place to edit a skill. plugins/rdra-ish/skills/ is a copy: a
# marketplace install copies only the plugin directory, and a symlink in its place breaks
# on a checkout without symlinks (Windows) and fails `claude plugin validate --strict`.
# Each skills/<name>/ with a SKILL.md replaces plugins/rdra-ish/skills/<name>/, and nothing
# else stays there. The plugin_manifest test fails while the two differ; the pre-commit hook
# runs this script.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
dest=plugins/rdra-ish/skills
rm -rf "$dest"
mkdir -p "$dest"
for skill in skills/*/; do
  [[ -f "${skill}SKILL.md" ]] || continue
  cp -R "${skill%/}" "$dest/"
done
