# Maintainer / Operator only: Stage 0 Docker deployment

This page is not a public quickstart. Ordinary users must use the downloadable local image bundle; MiniJAM does not distribute official Authority, Worker, Relayer, Sudo, or Faucet credentials.

The complete MiniJAM stack cannot currently be built independently from the public source repository because the Node, Runtime, Worker, and execution path depend on a pinned private Jambda revision.

The supported distribution and deployment path is the published Stage 0 Docker release. The target host does not need Rust, Cargo, LLVM, Node.js, or access to the Jambda source.

:::warning Operator credentials are required

The Stage 0 release stack is not a credential-free local development chain. The Authority keystore, three Worker seeds, and Playground Relayer URI must match the public identities in the release chain spec. Arbitrary replacement keys will cause the Node, Workers, or Playground to remain unhealthy.

Credentials are provisioned only in the maintainer-owned release environment and must never be requested, shared, or recreated by ordinary users.

:::

## What the stack runs

The release Compose stack starts one MiniJAM Node, one Compiler API, one Playground API and bundle gateway, three independently keyed Workers, and one Playground Web frontend. Playground API is intended to be publicly callable by browser clients; Node RPC, Compiler API, Worker health endpoints, and metrics remain private.

The MiniJAM Playground API is a public developer API and intentionally supports cross-origin browser clients. Its CORS policy is permissive; CORS is not an authorization boundary. Mutation authorization remains enforced by signed actions, sr25519 wallet signatures, replay protection, and Service-defined management policy where applicable.

## Requirements

Install Docker Engine, the Docker Compose plugin, Git, and `sha256sum`:

```bash
docker --version
docker compose version
git --version
sha256sum --version
```

Docker is the current supported full-Stack path. Do not use the public source repository to build the complete Node, Runtime, Worker, or execution stack.

## 1. Check out the matching release

Use the exact release tag associated with the bundle. The tag is a placeholder here because this documentation does not publish a release tag:

```bash
git clone --branch <RELEASE_TAG> --depth 1 \
  https://github.com/ArcheLabs/minijam-client.git
cd minijam-client
```

Do not deploy `main` with artifacts from a different release.

## 2. Obtain one complete release bundle

Obtain these files from the same successful Stage 0 release:

```text
release-manifest.json
stage0-raw.json
SHA256SUMS
```

The manifest is the source of truth for the MiniJAM commit, pinned Jambda commit, genesis hash, public identities, and all five image digests. Never combine a manifest, chain spec, image digest, or database from different releases.

```bash
cp /path/to/release-bundle/stage0-raw.json chain-specs/stage0-raw.json
cd /path/to/release-bundle
sha256sum -c SHA256SUMS
cd /path/to/minijam-client
```

The raw chain spec, genesis hash, five image digests, and credentials must all come from the same release bundle.

## 3. Configure the untracked environment file

```bash
cp deploy/stage0/.env.example deploy/stage0/.env
```

Replace every placeholder in `deploy/stage0/.env`. The five image values must be immutable digest references copied from `release-manifest.json`:

```dotenv
MINIJAM_NODE_IMAGE=ghcr.io/archelabs/minijam-node@sha256:<digest>
MINIJAM_WORKER_IMAGE=ghcr.io/archelabs/minijam-worker@sha256:<digest>
MINIJAM_COMPILER_IMAGE=ghcr.io/archelabs/minijam-compiler-api@sha256:<digest>
MINIJAM_PLAYGROUND_API_IMAGE=ghcr.io/archelabs/minijam-playground-api@sha256:<digest>
MINIJAM_PLAYGROUND_WEB_IMAGE=ghcr.io/archelabs/minijam-playground-web@sha256:<digest>
```

Set the matching chain values and release-specific Relayer URI:

```dotenv
MINIJAM_CHAIN_SPEC_PATH=./chain-specs/stage0-raw.json
MINIJAM_GENESIS_HASH=<genesis-hash-from-release-manifest>
MINIJAM_RELAYER_URI=<release-specific-relayer-uri>
MINIJAM_WEB_BIND=127.0.0.1
MINIJAM_WEB_PORT=4173
```

Keep `.env` untracked. Do not replace digest references with mutable tags.

## 4. Provision matching runtime credentials

Create these files using the credentials supplied for the same release:

```text
deploy/stage0/secrets/node-keystore.tar.gz
deploy/stage0/secrets/worker-1.seed
deploy/stage0/secrets/worker-2.seed
deploy/stage0/secrets/worker-3.seed
```

The Node archive must contain the Aura and GRANDPA keystore files matching the Authority identity in the chain spec. Each Worker seed must match its registered Worker session key. The Playground Relayer URI must also match the release deployment. A user without matching credentials should use a hosted Playground when one is available, rather than generating arbitrary seeds.

Set the secret paths in `.env`:

```dotenv
NODE_KEY_OR_SEED_PATH=./deploy/stage0/secrets/node-keystore.tar.gz
WORKER_1_SEED_PATH=./deploy/stage0/secrets/worker-1.seed
WORKER_2_SEED_PATH=./deploy/stage0/secrets/worker-2.seed
WORKER_3_SEED_PATH=./deploy/stage0/secrets/worker-3.seed
```

Apply restrictive permissions:

```bash
sudo chown 10001:10001 deploy/stage0/secrets/node-keystore.tar.gz \
  deploy/stage0/secrets/worker-1.seed deploy/stage0/secrets/worker-2.seed \
  deploy/stage0/secrets/worker-3.seed
sudo chmod 0400 deploy/stage0/secrets/node-keystore.tar.gz \
  deploy/stage0/secrets/worker-1.seed deploy/stage0/secrets/worker-2.seed \
  deploy/stage0/secrets/worker-3.seed
```

Never commit or paste credentials into Compose, source control, logs, screenshots, or support tickets.

## 5. Validate and pull the release

Resolve all environment interpolation before pulling images:

```bash
docker compose --env-file deploy/stage0/.env \
  -f compose.stage0.yml config >/dev/null
```

Do not continue if a variable is missing, the Relayer URI is empty, an image digest is a placeholder or tag, a reference is invalid, or a secret file is missing.

Pull the five immutable images:

```bash
docker compose --env-file deploy/stage0/.env \
  -f compose.stage0.yml pull
```

## 6. Start and verify health

```bash
docker compose --env-file deploy/stage0/.env \
  -f compose.stage0.yml up -d
docker compose --env-file deploy/stage0/.env \
  -f compose.stage0.yml ps
```

Wait until `node`, `compiler-api`, `playground-api`, `worker-1`, `worker-2`, `worker-3`, and `playground-web` are healthy. Inspect logs when needed:

```bash
docker compose --env-file deploy/stage0/.env \
  -f compose.stage0.yml logs --tail=200 node playground-api \
  worker-1 worker-2 worker-3
```

## 7. Open the Playground

After the Docker deployment is healthy, the default local address is [http://127.0.0.1:4173](http://127.0.0.1:4173). A hosted public Playground URL should be listed separately when one is deployed; this document does not invent one.

The browser may access Playground Web or the public Playground API directly. Do not expose Node RPC, Compiler API, Worker health endpoints, or metrics ports. For a public host, use an operator-managed HTTPS reverse proxy for the Web port and Playground API.

## 8. Stop, restart, or reset

Stop without deleting state:

```bash
docker compose --env-file deploy/stage0/.env \
  -f compose.stage0.yml down
```

Start again with the same release:

```bash
docker compose --env-file deploy/stage0/.env \
  -f compose.stage0.yml up -d
```

Reset Stage 0 and delete Node, Playground, bundle, and Worker state:

```bash
docker compose --env-file deploy/stage0/.env \
  -f compose.stage0.yml down --volumes --remove-orphans
```
