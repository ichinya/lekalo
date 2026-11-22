#!/usr/bin/env bash
# Verify this project's native runtime and its rules with no Node in PATH.
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
lekalo=${LEKALO_BIN:-"$root/target/debug/lekalo"}
checker=$(command -v lekalo-rust-check)
scratch=$(mktemp -d "${BB_THREAD_STORAGE:-${TMPDIR:-/tmp}}/lekalo-self-runtime.XXXXXXXX")
trap 'rm -rf -- "$scratch"' EXIT

project="$scratch/project"
component=crates/lekalo-core/src/runtime
rules=lekalo/rules/runtime-protocol
mkdir -p "$scratch/bin" "$project/$component" "$project/$rules"
install -m 0755 "$checker" "$scratch/bin/lekalo-rust-check"
cp "$root/lekalo.yaml" "$project/lekalo.yaml"
cp "$root/$rules/"*.json "$project/$rules/"
cp -R "$root/$component/." "$project/$component/"

run() {
  PATH="$scratch/bin" LEKALO_PACKAGE_HOME="$scratch/package-cache" \
    "$lekalo" --json "$@" --project "$project"
}

run runtime lock > "$scratch/lock.json"
jq -e '.lock.components["runtime-protocol"].runtime.description.id == "lekalo-rust"' "$scratch/lock.json" >/dev/null
run runtime lock --offline > /dev/null
run validate --strict > "$scratch/validate.json"
jq -e '.status == "valid" and .components[0].component == "runtime-protocol"' "$scratch/validate.json" >/dev/null

for declaration in Response ResultRecord; do
  awk -v declaration="$declaration" '
    previous ~ /^[[:space:]]*#\[serde\(deny_unknown_fields\)\][[:space:]]*$/ &&
      $0 ~ "^[[:space:]]*struct " declaration "[[:space:]]*[{]" {
        removed++; previous = $0; next
      }
    NR > 1 { print previous }
    { previous = $0 }
    END { print previous; if (removed != 1) exit 1 }
  ' "$root/$component/mod.rs" > "$project/$component/mod.rs"

  status=0
  run validate --strict > "$scratch/out.json" 2> "$scratch/refusal.json" || status=$?
  if [[ "$status" != 1 ]]; then
    cat "$scratch/out.json" "$scratch/refusal.json" >&2
    exit 1
  fi
  jq -e 'any(.diagnostics[];
    .id == "runtime.rule-failed" and
    .data.component == "runtime-protocol" and
    .data.rule == "strict-runtime-response")' "$scratch/refusal.json" >/dev/null
done
