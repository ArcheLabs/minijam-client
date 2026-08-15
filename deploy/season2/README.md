# Season 2 deployment profiles

Season 2 is an Experience Network. The compact profile runs one node, one
worker, the Experience API, compiler, and allocation submitter on one host
while keeping the node, API, and compiler on separate Docker network surfaces.
The split profile uses the same runtime and separates the node/worker host from
the API/compiler host over a private `node-net` connection.

Both profiles use `--rpc-methods=safe`. The compiler has no validator,
allocation-relayer, worker, or sudo key mounts. Ingress and Allocation Relayer
credentials are separate and belong only to the API/submitter process; the
worker receives only its own signing secret. Configure production images and
secrets through environment variables; the example values are placeholders
only.

Generate a fresh chain spec for each release. The generated files are not
committed because the public relayer identities are deployment inputs:

```bash
cargo build --release -p minijam-node
MINIJAM_SEASON2_INGRESS_RELAYER_PUBLIC_KEY=0x... \
MINIJAM_SEASON2_ALLOCATION_RELAYER_PUBLIC_KEY=0x... \
  ./scripts/export-season2-chain-specs.sh
```

The full compact smoke command is:

```bash
MINIJAM_WORKER_KEY_FILE=./secrets/worker.seed \
MINIJAM_SEASON2_CHAIN_SPEC_FILE=./chain-specs/season2.json \
  ./scripts/test-season2-e2e.sh
```

It starts from empty volumes, exercises controller and non-controller Work,
Preimage, two allocations, duplicate delivery, and node/worker/API restart.
The worker health endpoint is `/health/ready` (also `/readyz`); API and
compiler expose `/health/live`/`/health/ready` (also `/healthz`/`/readyz`).

The runtime does not expose a MiniJAM-to-Hub release operation. Allocation
redemption remains entirely a Hub concern. The external Allocation relay must
consume finalized Hub events, persist a replay-safe `(block,event_index)`
cursor, and retry the same `allocation_id` until the API accepts it. It may
use only `MINIJAM_ALLOCATION_RELAYER_URI`; it must not receive validator,
worker, sudo, or Hub-redemption credentials.
