#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
# shellcheck source=stage1-core-packages.sh
source "${ROOT}/scripts/stage1-core-packages.sh"

if (( ${#STAGE1_CORE_PACKAGES[@]} == 0 )); then
  echo "Stage-1 core package manifest must not be empty" >&2
  exit 1
fi

for forbidden in minijam-compiler-api minijam-playground-api; do
  if printf '%s\n' "${STAGE1_CORE_PACKAGES[@]}" | grep -Fxq "${forbidden}"; then
    echo "Stage-1 core package manifest contains external product package: ${forbidden}" >&2
    exit 1
  fi
done

unique_count="$(printf '%s\n' "${STAGE1_CORE_PACKAGES[@]}" | sort -u | wc -l)"
if (( unique_count != ${#STAGE1_CORE_PACKAGES[@]} )); then
  echo "Stage-1 core package manifest contains duplicates" >&2
  exit 1
fi

core_json="$(printf '%s\n' "${STAGE1_CORE_PACKAGES[@]}" | jq -R . | jq -s .)"
metadata="$(cargo metadata --locked --manifest-path "${ROOT}/Cargo.toml" --format-version 1)"

missing="$(jq -r --argjson core "${core_json}" '($core - [.packages[].name])[]?' <<<"${metadata}")"
if [[ -n "${missing}" ]]; then
  printf 'Stage-1 core package manifest contains unknown package(s):\n%s\n' "${missing}" >&2
  exit 1
fi

violations="$(jq -r --argjson core "${core_json}" '
  .resolve.nodes as $nodes_list
  | ($nodes_list | map({key: .id, value: .}) | from_entries) as $nodes
  | (.packages | map(select(.name == "clang-sys") | .id) | first) as $clang
  | def reachable($root):
      {seen: [], queue: [$root]}
      | until((.queue | length) == 0;
          .queue[0] as $current
          | .queue = .queue[1:]
          | if (($current as $c | .seen | index($c)) != null) then .
            else
              .seen += [$current]
              | .queue += [$nodes[$current].deps[]?.pkg]
            end)
      | .seen;
  .packages[]
  | .name as $name
  | select(($core | index($name)) != null)
  | select((reachable(.id) | index($clang)) != null)
  | $name
' <<<"${metadata}")"

if [[ -n "${violations}" ]]; then
  printf 'Stage-1 core package(s) reach clang-sys:\n%s\n' "${violations}" >&2
  exit 1
fi

printf 'STAGE1_CORE_DEPENDENCY_BOUNDARY=PASS\n'
