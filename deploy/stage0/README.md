# MiniJAM Stage 0 release stack

The release stack reuses the M9 topology: one Node, one Compiler API, one
Playground API (including the bundle gateway), three independent Workers, and
one production Playground Web. It pulls five public MiniJAM images and does not
build source code.

Only Playground Web is published by default. Node RPC, Compiler API,
Playground API, bundle routes, and Worker health endpoints remain on the
private Compose network. Web proxies `/api/` and `/ipfs/` to Playground API.

## Requirements

- Docker Engine with the Compose plugin;
- a Stage 0 release manifest and its matching `stage0-raw.json`;
- runtime credentials matching the public authority, relayer, and Worker
  identities in that chain spec.

Rust, Cargo, LLVM, Node.js, and access to the Jambda source repository are not
required.

## Configure

Copy the public template:

```bash
cp deploy/stage0/.env.example deploy/stage0/.env
```

Fill every image value from the `images` section of the matching
`release-manifest.json`. Use the complete
`ghcr.io/archelabs/<name>@sha256:<digest>` reference, never a mutable tag.
Set `MINIJAM_GENESIS_HASH` from the same manifest and keep the matching raw
chain spec at `chain-specs/stage0-raw.json`.

Provision runtime credentials as described in
`deploy/stage0/secrets/README.md`. The `.env` file and all files below the
secrets directory are ignored by Git. Do not copy credentials into the Compose
file or an image.

The release chain spec contains three authority identities. A single local Node
can run the complete resettable chain when its external keystore contains all
matching Aura and GRANDPA keys. A public testnet operator may instead deploy
one Node per authority using the same Node image and raw chain spec.

## Pull and start

Run the release stack from the repository root:

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

Wait until all seven services are healthy:

```bash
docker compose \
  --env-file deploy/stage0/.env \
  -f compose.stage0.yml \
  ps
```

Open `http://127.0.0.1:4173` unless `MINIJAM_WEB_BIND` or
`MINIJAM_WEB_PORT` was changed. For an explicitly public Stage 0 host, set
`MINIJAM_WEB_BIND=0.0.0.0` and put an operator-managed TLS reverse proxy in
front of this single port.

## Reset

Stage 0 is intentionally resettable. This command deletes Node, Playground,
bundle, and Worker state volumes:

```bash
docker compose \
  --env-file deploy/stage0/.env \
  -f compose.stage0.yml \
  down --volumes --remove-orphans
```

The chain can only be restarted from the same genesis by reusing the exact raw
chain spec and release manifest. Do not combine databases, binaries, images, or
chain specs from different releases.

## Development stack

Repository contributors continue to use the M9 build-capable topology:

```bash
MINIJAM_IMAGE_TAG="$(git rev-parse HEAD)" \
  docker compose -f compose.dev.yml build
```

`compose.dev.yml` is not a release deployment input.
