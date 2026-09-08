# Stage-1 Docker deployment

Stage-1 is the supported MiniJAM deployment. The release unit is a set of
immutable image digests plus the chain specification generated from the exact
node image. The repository does not contain a separate local, Stage-0, or web
deployment stack.

## Components

The compact and split profiles contain the same three roles:

- `node`: validator and safe JSON-RPC endpoint;
- `worker`: Worker daemon with its own signing key and state volume;
- `formal-rpc`: application-neutral Work and bundle gateway with the Work-ingress
  relayer key and bundle volume.

The optional Service compiler is a separate image from `deploy/compiler` and is
not part of the Stage-1 runtime network.

## Prepare release artifacts

Obtain the three image digests from the Stage-1 release artifact. Generate the
matching chain specs from the exact node image:

```bash
MINIJAM_NODE_IMAGE=ghcr.io/archelabs/minijam-node@sha256:<digest> \
MINIJAM_STAGE1_CHAIN_SPEC_DIR=./chain-specs \
./scripts/export-stage1-chain-specs-image.sh
```

The generated files are:

```text
chain-specs/stage1.json
chain-specs/stage1-raw.json
```

They must match the release manifest hashes and must not be mixed with another
node image.

## Compact deployment

Set the image references, generated chain spec, and secret values, then
validate and start the stack. Compose exposes these environment-backed values
to each non-root container as `0400` `/run/secrets/*` files; they are not
injected into the application environment:

```bash
export MINIJAM_NODE_IMAGE=ghcr.io/archelabs/minijam-node@sha256:<digest>
export MINIJAM_WORKER_IMAGE=ghcr.io/archelabs/minijam-worker@sha256:<digest>
export MINIJAM_FORMAL_RPC_IMAGE=ghcr.io/archelabs/minijam-formal-rpc@sha256:<digest>
export MINIJAM_STAGE1_CHAIN_SPEC_FILE=./chain-specs/stage1.json
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
same image digests, generated chain spec, and secret conventions in
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
