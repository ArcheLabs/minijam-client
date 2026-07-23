# MiniJAM Stage-0 Deployment

This directory contains operator templates for a public Stage-0 testnet.

## Artifacts

Build and export before deployment:

```bash
cargo build --release -p minijam-node -p minijam-worker
./scripts/export-stage0-chain-specs.sh ./target/release/minijam-node chain-specs
```

Publish these files to every node host:

- `target/release/minijam-node`
- `target/release/minijam-worker`
- `chain-specs/stage0-raw.json`

## Topology

- 3 authority nodes run `minijam-node --validator --chain stage0-raw.json`.
- 1 or more public RPC full nodes run without `--validator`.
- 3 worker daemons connect to finalized chain state through the public RPC or a private full node.
- Prometheus scrapes node and worker hosts through private networking.

Authority nodes must not expose RPC to the public internet. Public RPC nodes should expose only safe methods and should not hold authority session keys.

## Docker Compose

`docker-compose.yml` is a topology template. It expects these images unless overridden:

- `MINIJAM_NODE_IMAGE=ghcr.io/archelabs/minijam-node:stage0`
- `MINIJAM_WORKER_IMAGE=ghcr.io/archelabs/minijam-worker:stage0`

It also expects:

- `../../chain-specs/stage0-raw.json`
- `./secrets/authority-*/node-key`
- `.env` copied from `.env.example`, with `AUTHORITY_1_PEER_ID` set after generating the authority-1 node key.
- `./worker-*.toml`, edited with real worker keys when candidate submission is enabled.

Start a single-host smoke topology:

```bash
docker compose -f deploy/stage0/docker-compose.yml up -d
```

## Systemd

Install binaries under `/usr/local/bin`, chain spec under `/etc/minijam/stage0-raw.json`, and data under `/var/lib/minijam`.

Authority:

```bash
sudo install -m 0644 deploy/stage0/systemd/minijam-authority.service /etc/systemd/system/
sudo systemctl enable --now minijam-authority
```

Public RPC:

```bash
sudo install -m 0644 deploy/stage0/systemd/minijam-public-rpc.service /etc/systemd/system/
sudo systemctl enable --now minijam-public-rpc
```

Worker:

```bash
sudo install -m 0644 deploy/stage0/systemd/minijam-worker.service /etc/systemd/system/
sudo systemctl enable --now minijam-worker
```

## Public RPC Safety Profile

Use a public RPC node, not an authority node, for internet-facing JSON-RPC:

```bash
minijam-node \
  --chain /etc/minijam/stage0-raw.json \
  --name stage0-public-rpc-1 \
  --base-path /var/lib/minijam/public-rpc \
  --rpc-external \
  --rpc-methods safe \
  --rpc-rate-limit 600 \
  --rpc-max-request-size 8 \
  --rpc-max-response-size 64 \
  --rpc-cors https://polkadot.js.org \
  --prometheus-external
```

Do not pass `--unsafe-rpc-external` or `--rpc-methods unsafe` on public nodes.

Authority RPC should stay loopback-only:

```bash
minijam-node \
  --chain /etc/minijam/stage0-raw.json \
  --validator \
  --rpc-methods safe
```

Expose authority metrics only on a private network or through a node-local collector.
