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

mkdir -p "${output}/node-keystore"
chmod 700 "${output}" "${output}/node-keystore"

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

for index in 1 2 3; do
  generate_seed "${output}/authority-${index}-aura.seed"
  generate_seed "${output}/authority-${index}-grandpa.seed"
  generate_seed "${output}/worker-${index}.seed"
done
generate_seed "${output}/e2e-wallet.seed"

authorities='[]'
workers='[]'
for index in 1 2 3; do
  aura="$(public_key sr25519 "${output}/authority-${index}-aura.seed")"
  grandpa="$(public_key ed25519 "${output}/authority-${index}-grandpa.seed")"
  worker="$(public_key sr25519 "${output}/worker-${index}.seed")"
  authorities="$(
    jq -c \
      --arg aura "${aura}" \
      --arg grandpa "${grandpa}" \
      '. + [{aura: $aura, grandpa: $grandpa}]' \
      <<<"${authorities}"
  )"
  workers="$(
    jq -c \
      --arg account "${worker}" \
      '. + [{account: $account, session_key: $account}]' \
      <<<"${workers}"
  )"

  "${node}" key insert \
    --keystore-path "${output}/node-keystore" \
    --key-type aura \
    --scheme sr25519 \
    --suri "${output}/authority-${index}-aura.seed" \
    >/dev/null
  "${node}" key insert \
    --keystore-path "${output}/node-keystore" \
    --key-type gran \
    --scheme ed25519 \
    --suri "${output}/authority-${index}-grandpa.seed" \
    >/dev/null
done

jq -n \
  --argjson authorities "${authorities}" \
  --argjson workers "${workers}" \
  '{authorities: $authorities, workers: $workers}' \
  >"${output}/public-identities.json"

tar -czf "${output}/node-keystore.tar.gz" -C "${output}/node-keystore" .
base64 -w0 "${output}/node-keystore.tar.gz" >"${output}/node-keystore.b64"
printf '\n' >>"${output}/node-keystore.b64"
chmod 600 "${output}"/*.seed "${output}/node-keystore.tar.gz" "${output}/node-keystore.b64"

echo "Generated private Stage 0 credentials in the ignored directory:"
echo "${output}"
echo "Only public-identities.json may be used as source for committed public keys."
