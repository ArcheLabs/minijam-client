# Minimal Worker deployment

M7 uses one `minijam-worker` image for every identity. Run three containers
(`worker-1`, `worker-2`, and `worker-3`) with distinct `WORKER_ID` values,
seed-file mounts, and `/data` volumes. They may share the Node RPC and Bundle
gateway.

The image runs as UID/GID `10001`, never embeds a signing seed, and expects a
read-only secret file:

```text
MINIJAM_WORKER_SEED_FILE=/run/secrets/worker-seed
```

Copy `worker.example.toml` for each instance and set the real finalized genesis
hash. `WORKER_ID`, `NODE_RPC_URL`, `BUNDLE_GATEWAY_URL`, and `POLL_INTERVAL`
environment variables override the file. `WORKER_SIGNING_KEY` is supported for
development, but a mounted seed file is preferred.

Each instance needs its own:

- registered Worker ID and matching session signing key;
- writable `/data` volume;
- container/log identity;
- transaction account and nonce stream.

The Worker recovers from finalized chain tasks. A Candidate that already exists
is absent from pending Candidate tasks, and a Worker ID already present in
`submitted_votes` is skipped. The local recovery file is only an optimization,
not the source of truth.

Health endpoints are served on port `8082` by default:

- `GET /health/live` confirms the process is serving requests.
- `GET /health/ready` returns 200 only after a signing identity is loaded, the
  Node genesis matches configuration, and the Bundle gateway readiness endpoint
  is reachable.

Full Compose wiring and the browser-to-accumulate end-to-end flow remain M9
work.
