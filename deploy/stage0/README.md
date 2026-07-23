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

## Backup and Restore

Back up each host before runtime upgrades, validator key rotation, or storage maintenance. Keep authority secrets out of shared operator channels.

Systemd host backup:

```bash
sudo systemctl stop minijam-authority minijam-public-rpc minijam-worker
sudo install -d -m 0700 /var/backups/minijam
sudo tar -C / -czf /var/backups/minijam/stage0-$(date -u +%Y%m%dT%H%M%SZ).tgz \
  etc/minijam \
  var/lib/minijam \
  usr/local/bin/minijam-node \
  usr/local/bin/minijam-worker
sudo systemctl start minijam-authority minijam-public-rpc minijam-worker
```

Docker Compose backup:

```bash
docker compose -f deploy/stage0/docker-compose.yml stop
tar -czf stage0-compose-$(date -u +%Y%m%dT%H%M%SZ).tgz \
  chain-specs/stage0-raw.json \
  deploy/stage0/*.toml \
  deploy/stage0/secrets \
  deploy/stage0/.env
docker run --rm \
  -v minijam-client_authority-1-data:/authority-1:ro \
  -v minijam-client_authority-2-data:/authority-2:ro \
  -v minijam-client_authority-3-data:/authority-3:ro \
  -v minijam-client_public-rpc-data:/public-rpc:ro \
  -v minijam-client_worker-1-data:/worker-1:ro \
  -v minijam-client_worker-2-data:/worker-2:ro \
  -v minijam-client_worker-3-data:/worker-3:ro \
  -v "$PWD:/backup" alpine \
  tar -czf /backup/stage0-volumes-$(date -u +%Y%m%dT%H%M%SZ).tgz \
    /authority-1 /authority-2 /authority-3 /public-rpc /worker-1 /worker-2 /worker-3
docker compose -f deploy/stage0/docker-compose.yml start
```

Restore onto a replacement systemd host:

```bash
sudo systemctl stop minijam-authority minijam-public-rpc minijam-worker || true
sudo tar -C / -xzf /var/backups/minijam/stage0-YYYYMMDDTHHMMSSZ.tgz
sudo systemctl daemon-reload
sudo systemctl start minijam-authority minijam-public-rpc minijam-worker
```

Restore Compose state by stopping the stack, restoring `chain-specs/`, `deploy/stage0/`, and the Docker volumes from the matching archives, then starting the stack again. Do not mix node databases from one chain spec with a different `stage0-raw.json`.
