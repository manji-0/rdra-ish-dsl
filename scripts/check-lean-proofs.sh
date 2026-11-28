#!/usr/bin/env bash
# Build the Lean proofs under proofs/lean and check that no headline theorem rests on
# `sorry` or an axiom of its own. `lake build` alone passes with a `sorry` (it only warns),
# so the check reads `#print axioms` of each theorem CheckAxioms.lean lists.
#
#   scripts/check-lean-proofs.sh
#
# Needs `lake` on PATH (elan installs it and the toolchain proofs/lean/lean-toolchain pins;
# `nix develop` provides elan, and `nix shell nixpkgs#elan` does too).
set -euo pipefail
cd "$(dirname "$0")/../proofs/lean"

if grep -rnwE 'sorry|admit|axiom' --include='*.lean' RdraIsh RdraIsh.lean; then
  echo "check-lean-proofs: a sorry, admit or axiom in the sources (above)" >&2
  exit 1
fi

lake build
out=$(lake env lean CheckAxioms.lean)
echo "$out"
want=$(grep -c '^#print axioms' CheckAxioms.lean)
got=$(grep -c "^'RdraIsh\." <<<"$out" || true)
if [[ "$got" != "$want" ]]; then
  echo "check-lean-proofs: $want theorems listed, $got reported" >&2
  exit 1
fi
bad=$(grep "^'RdraIsh\." <<<"$out" | grep -v "does not depend on any axioms" |
  grep -vE "depends on axioms: \[(propext|Classical\.choice|Quot\.sound)(, (propext|Classical\.choice|Quot\.sound))*\]$" || true)
if [[ -n "$bad" ]]; then
  echo "check-lean-proofs: theorems resting on other axioms:" >&2
  echo "$bad" >&2
  exit 1
fi
echo "check-lean-proofs: $got theorems, standard axioms only"
