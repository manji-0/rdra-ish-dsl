#!/usr/bin/env bash
# Check that a release tag names the version the tree builds:
#
#   scripts/check-release-version.sh vX.Y.Z
#
# The tag without its `v`, `version` under `[workspace.package]` in Cargo.toml (the version
# of every crate and of the `rdra-ish` binary), and `version` under `[project]` in
# pyproject.toml (the PyPI package's) must be the same string. release.yml and
# publish-pypi.yml run it before they build anything.
set -euo pipefail

tag="${1:?usage: check-release-version.sh vX.Y.Z}"
cd "$(dirname "${BASH_SOURCE[0]}")/.."

python3 -I - "$tag" <<'EOF'
import sys
import tomllib

tag = sys.argv[1]
with open("Cargo.toml", "rb") as f:
    cargo = tomllib.load(f)["workspace"]["package"]["version"]
with open("pyproject.toml", "rb") as f:
    pyproject = tomllib.load(f)["project"]["version"]
want = tag[1:] if tag.startswith("v") else None
print(f"tag {tag}, Cargo.toml {cargo}, pyproject.toml {pyproject}")
if want is None:
    sys.exit(f"the tag {tag} does not start with v")
if not (want == cargo == pyproject):
    sys.exit(f"the versions differ: tag {want}, Cargo.toml {cargo}, pyproject.toml {pyproject}")
EOF
