#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

step6_sample="samples/incremental-order/step-6-business-rules"
step6_src="$step6_sample/src"
step6_out="$step6_sample/out"
api_contract_sample="samples/api-contract"
api_contract_src="$api_contract_sample/src"
api_contract_out="$api_contract_sample/out"

run_rdra_ish() {
  cargo run --quiet --bin rdra-ish -- "$@"
}

mkdir -p "$api_contract_out"

run_rdra_ish diagram "$step6_src" --kind rdra --format mermaid --buc BucStoreRestock --out "$step6_out/object_graph_buc_store_restock"
run_rdra_ish diagram "$step6_src" --kind sequence --format mermaid --buc BucStoreRestock --out "$step6_out/sequence_buc_store_restock"
run_rdra_ish diagram "$step6_src" --kind er --format mermaid --out "$step6_out/er"
run_rdra_ish diagram "$step6_src" --kind er --format puml --out "$step6_out/er"
run_rdra_ish diagram "$step6_src" --kind state --format mermaid --out "$step6_out/state"
run_rdra_ish csv "$step6_src" --kind matrix --out "$step6_out/usecase_matrix.csv"
run_rdra_ish csv "$step6_src" --kind screen-constraints --out "$step6_out/screen_constraints.csv"
run_rdra_ish csv "$step6_src" --kind actor-permission-audit --out "$step6_out/actor_permission_audit.csv"
run_rdra_ish states "$step6_src" --format table --entity Store > "$step6_out/states_store.txt"
run_rdra_ish export "$step6_src" --kind dbml --out "$step6_out/schema.dbml"
run_rdra_ish export "$step6_src" --kind json-schema --out "$step6_out/json-schema.json"
run_rdra_ish export "$step6_src" --kind asyncapi --out "$step6_out/asyncapi.json"
run_rdra_ish export "$api_contract_src" --kind openapi --out "$api_contract_out/openapi.json"

if ! git diff --quiet -- "$step6_out" "$api_contract_out"; then
  echo "sample artifacts changed; review and commit the generated diff if intentional" >&2
  git --no-pager diff -- "$step6_out" "$api_contract_out" >&2
  exit 1
fi

untracked="$(git ls-files --others --exclude-standard -- "$step6_out" "$api_contract_out")"
if [[ -n "$untracked" ]]; then
  if [[ "${CI:-false}" == "true" ]]; then
    echo "sample artifact generation produced untracked files:" >&2
    echo "$untracked" >&2
    exit 1
  fi
  echo "warning: sample artifact generation produced untracked files:" >&2
  echo "$untracked" >&2
fi

echo "sample artifacts are up to date"
