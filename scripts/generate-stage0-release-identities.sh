#!/usr/bin/env bash
set -euo pipefail

repository="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
node="${repository}/target/release/minijam-node"
output="${repository}/deploy/stage0/secrets/generated"

test -x "${node}" || {
  echo "build target/release/minijam-node before generating release identities" >&2
  exit 1
}
if [[ -e "${output}" ]]; then
  echo "refusing to replace existing generated Stage 0 credentials: ${output}" >&2
  exit 1
fi

mkdir -p "${output}/node-runtime/keystore"
chmod 700 "${output}" "${output}/node-runtime" "${output}/node-runtime/keystore"

generate_seed() {
  local path="$1"
  umask 077
  printf '0x%s\n' "$(openssl rand -hex 32)" >"${path}"
}

public_key() {
  local scheme="$1"
  local seed_file="$2"
  local inspect
  inspect="$(mktemp)"
  "${node}" key inspect --scheme "${scheme}" "${seed_file}" >"${inspect}"
  awk '/Public key \(hex\):/ { print $4; found = 1 } END { exit !found }' "${inspect}"
}

generate_seed "${output}/authority-1-aura.seed"
generate_seed "${output}/authority-1-grandpa.seed"
for index in 1 2 3; do
  generate_seed "${output}/worker-${index}.seed"
done
generate_seed "${output}/e2e-wallet.seed"

authorities='[]'
workers='[]'
aura="$(public_key sr25519 "${output}/authority-1-aura.seed")"
grandpa="$(public_key ed25519 "${output}/authority-1-grandpa.seed")"
authorities="$(
  jq -c \
    --arg aura "${aura}" \
    --arg grandpa "${grandpa}" \
    '. + [{aura: $aura, grandpa: $grandpa}]' \
    <<<"${authorities}"
)"
"${node}" key insert \
  --keystore-path "${output}/node-runtime/keystore" \
  --key-type aura \
  --scheme sr25519 \
  --suri "${output}/authority-1-aura.seed" \
  >/dev/null
"${node}" key insert \
  --keystore-path "${output}/node-runtime/keystore" \
  --key-type gran \
  --scheme ed25519 \
  --suri "${output}/authority-1-grandpa.seed" \
  >/dev/null

for index in 1 2 3; do
  worker="$(public_key sr25519 "${output}/worker-${index}.seed")"
  workers="$(
    jq -c \
      --arg account "${worker}" \
      '. + [{account: $account, session_key: $account}]' \
      <<<"${workers}"
  )"
done

"${node}" key generate-node-key \
  --file "${output}/node-runtime/node-key" \
  2>/dev/null

jq -n \
  --argjson authorities "${authorities}" \
  --argjson workers "${workers}" \
  '{authorities: $authorities, workers: $workers}' \
  >"${output}/public-identities.json"

tar -czf "${output}/node-keystore.tar.gz" -C "${output}/node-runtime" .
base64 -w0 "${output}/node-keystore.tar.gz" >"${output}/node-keystore.b64"
printf '\n' >>"${output}/node-keystore.b64"
chmod 600 "${output}"/*.seed "${output}/node-keystore.tar.gz" "${output}/node-keystore.b64"

echo "Generated private Stage 0 credentials in the ignored directory:"
echo "${output}"
echo "Only public-identities.json may be used as source for committed public keys."
