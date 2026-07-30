# MiniJAM Stage 0 Human Playground Test Runbook

This runbook prepares a local human test environment for the fixed Stage 0 release:

- Release tag: `v0.1.0-stage0.1`
- MiniJAM commit: `bc0f0f20ee6d8086f5c1041c1294759b89f2b4fc`
- Genesis hash: `0x49a0833903d204f2658e1f19185d935500e1c16286ae16ffbc67eefad07f6d53`

The MiniJAM services must use public GHCR image digests from the matching
`release-manifest.json`. Do not use mutable image tags for the node, workers,
compiler API, Playground API, or Playground Web.

## Release Artifact

Download and unzip the release artifact named `minijam-v0.1.0-stage0.1`, then
install it into the local workspace:

```bash
./scripts/install-stage0-human-test-artifact.sh /path/to/unzipped/minijam-v0.1.0-stage0.1
```

The script verifies `SHA256SUMS`, checks the release tag, commit, and genesis
hash, copies the formal raw chain spec to `chain-specs/stage0-raw.json`, and
writes the non-secret part of `deploy/stage0/.env`.

## Secrets

Prepare these ignored local files:

```text
deploy/stage0/secrets/node-keystore.tar.gz
deploy/stage0/secrets/worker-1.seed
deploy/stage0/secrets/worker-2.seed
deploy/stage0/secrets/worker-3.seed
```

Fill the following values in `deploy/stage0/.env` without committing or printing
them:

```text
MINIJAM_RELAYER_URI=
FAUCET_ACCOUNT_MNEMONIC=
FAUCET_DB_PASSWORD=
RECAPTCHA_SECRET=
```

The expected public identities are recorded in `deploy/stage0/IDENTITIES.md`.
The faucet account must match the public key in the release manifest.

Set secret file permissions:

```bash
sudo chown 10001:10001 \
  deploy/stage0/secrets/node-keystore.tar.gz \
  deploy/stage0/secrets/worker-1.seed \
  deploy/stage0/secrets/worker-2.seed \
  deploy/stage0/secrets/worker-3.seed

sudo chmod 0400 \
  deploy/stage0/secrets/node-keystore.tar.gz \
  deploy/stage0/secrets/worker-1.seed \
  deploy/stage0/secrets/worker-2.seed \
  deploy/stage0/secrets/worker-3.seed
```

## Faucet Image

Build the MiniJAM-enabled faucet image from the sibling faucet repository:

```bash
cd /home/libingjiang/polkadot-testnet-faucet
docker build -t polkadot-testnet-faucet:minijam-local .
```

The MiniJAM release images are not rebuilt for this test.

## Preflight

Run:

```bash
cd /home/libingjiang/minijam-client
./scripts/prepare-stage0-human-test.sh
```

The script checks Docker/Compose availability, immutable GHCR digests, release
artifact checksums, genesis and public key consistency, secret permissions,
absence of Compose build sections, and local port availability.

## Start

```bash
docker compose \
  --env-file deploy/stage0/.env \
  -f compose.stage0.yml \
  pull

docker compose \
  --env-file deploy/stage0/.env \
  -f compose.stage0.yml \
  up -d
```

Open the Playground at:

```text
http://127.0.0.1:4173
```

The faucet API is bound locally at:

```text
http://127.0.0.1:5555
```

The browser-facing Playground Web only uses the Playground API and the Web
proxy path `/faucet/`.

## Human Flow

Use a real browser wallet account:

1. Connect the wallet.
2. Click `Get test MINI`.
3. Build the Counter C or C++ example.
4. Deploy the Service.
5. Submit Work and wait for Candidate, Vote, and Accumulate.
6. Open the Service page and verify finalized storage changed.
7. Build and submit an Upgrade.

The faucet request always uses the connected wallet address; there is no UI for
entering an arbitrary recipient address.

## Logs

```bash
docker compose --env-file deploy/stage0/.env -f compose.stage0.yml ps
docker compose --env-file deploy/stage0/.env -f compose.stage0.yml logs -f node
docker compose --env-file deploy/stage0/.env -f compose.stage0.yml logs -f playground-api
docker compose --env-file deploy/stage0/.env -f compose.stage0.yml logs -f faucet-api
docker compose --env-file deploy/stage0/.env -f compose.stage0.yml logs -f worker-1 worker-2 worker-3
```

## Reset

This removes local chain, Playground, worker, bundle, and faucet database
volumes. It does not delete release artifacts, `.env`, or secret files.

```bash
docker compose \
  --env-file deploy/stage0/.env \
  -f compose.stage0.yml \
  down --volumes --remove-orphans
```
