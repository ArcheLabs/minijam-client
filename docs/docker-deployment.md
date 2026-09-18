# Stage-1 Docker deployment

Stage-1 is the supported MiniJAM deployment. The release unit is a set of
immutable image digests plus the deterministic `testnet` chain specification
generated from the exact node image. Local development uses the aggregate
`minijam --dev` image and does not require Compose.

## Components

The compact and split testnet profiles contain the same three roles:

- `node`: validator and safe JSON-RPC endpoint;
- `worker`: Worker daemon with its own signing key and state volume;
- `formal-rpc`: application-neutral Work and bundle gateway with the Work-ingress
  relayer key and bundle volume.

The aggregate local image additionally contains the launcher, node, worker, and
Formal RPC. It starts exactly one Worker:

```bash
docker run --rm \
  -p 9944:9944 -p 8080:8080 \
  ghcr.io/archelabs/minijam@sha256:<digest> --dev
```

The optional Service compiler is a separate image from `deploy/compiler` and is
not part of the Stage-1 runtime network.

## Prepare release artifacts

Obtain the three component image digests from the Stage-1 release artifact.
Generate the matching testnet specs from the exact node image:

```bash
MINIJAM_NODE_IMAGE=ghcr.io/archelabs/minijam-node@sha256:<digest> \
MINIJAM_TESTNET_CHAIN_SPEC_DIR=./chain-specs \
./scripts/export-testnet-chain-specs-image.sh
```

The generated files are:

```text
chain-specs/testnet.json
chain-specs/testnet-raw.json
```

They must match the release manifest hashes and must not be mixed with another
node image.

## Compact deployment

Set the image references and secret values, then validate and start the stack.
The built-in `testnet` chain spec is selected by the node; no generated chain
spec is mounted. Compose exposes these environment-backed values
to each non-root container as `0400` `/run/secrets/*` files; they are not
injected into the application environment:

```bash
export MINIJAM_NODE_IMAGE=ghcr.io/archelabs/minijam-node@sha256:<digest>
export MINIJAM_WORKER_IMAGE=ghcr.io/archelabs/minijam-worker@sha256:<digest>
export MINIJAM_FORMAL_RPC_IMAGE=ghcr.io/archelabs/minijam-formal-rpc@sha256:<digest>
export MINIJAM_NODE_NETWORK_KEY=0x<64-hex-bytes>
export MINIJAM_WORKER_SEED=0x<64-hex-bytes>
export MINIJAM_FORMAL_RPC_RELAYER_URI=0x<64-hex-bytes>

docker compose -f deploy/stage1/compose.compact.yml config
docker compose -f deploy/stage1/compose.compact.yml up -d
docker compose -f deploy/stage1/compose.compact.yml ps
```

The compact profile publishes only the operator-selected Node RPC and Formal
RPC ports on loopback by default. Do not expose Worker health or metrics
endpoints publicly.

## Split deployment

Create one shared external `chain` network on the participating hosts. Use the
same image digests and secret conventions in
`deploy/stage1/compose.split.yml`; set `MINIJAM_RPC_URL` to the private Node
RPC address visible from the Formal RPC host.

```bash
docker network create chain
docker compose -f deploy/stage1/compose.split.yml config
docker compose -f deploy/stage1/compose.split.yml up -d
```

The node network key is also a deployment secret: it fixes the validator's
libp2p identity across restarts. The Worker and Formal RPC signing keys are
separate responsibilities. Never copy any of them into an image, commit them,
or reuse a development seed on a public network.

## Verification and teardown

The release gate runs Compose validation, candidate image smoke, node restart
recovery, Formal RPC readiness, Worker readiness, and artifact secret hygiene.
For an operator-managed deployment:

```bash
docker compose -f deploy/stage1/compose.compact.yml ps
docker compose -f deploy/stage1/compose.compact.yml logs --tail=200 node worker formal-rpc
docker compose -f deploy/stage1/compose.compact.yml down
```
