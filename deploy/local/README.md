# Stage 0 local full stack

This Compose project runs one deterministic development chain, the Compiler
API, the persistent Playground API, three independently keyed Workers, and the
browser Playground.

The browser publishes only port `4173` and reaches chain, compiler, Bundle, and
Worker functionality through the Playground API reverse proxy. Node port
`9944` is published for E2E assertions and local debugging, but it is never
configured in the Web build.

## Commands

From the repository root:

```bash
export MINIJAM_IMAGE_TAG=<trusted-build-git-sha>
./scripts/stage0-local.sh pull
./scripts/stage0-local.sh reset
./scripts/stage0-local.sh up
./scripts/stage0-local.sh test
./scripts/stage0-local.sh down
```

`reset` removes the local Node, Playground, and Worker volumes. Ordinary
`down` preserves them.

`build` packages existing host or CI release binaries into thin local images.
It does not invoke Cargo. Build these artifacts first in a stable environment:

```bash
cargo build --locked --release \
  -p minijam-node \
  -p minijam-compiler-api \
  -p minijam-playground-api \
  -p minijam-worker
cargo build --locked --release \
  --manifest-path service-toolchain/compiler/polkavm-to-jam/Cargo.toml
./scripts/stage0-local.sh build
```

This is an optional developer fallback, not an M9 acceptance or release build.
It tags the thin images using `MINIJAM_IMAGE_REGISTRY` and
`MINIJAM_IMAGE_TAG` (the current full Git SHA by default).

Runtime services use `pull_policy: never`, and `up` also passes
`--no-build --pull never`. Only the explicit `pull` command contacts the
registry. The registry defaults to `ghcr.io/archelabs`.
Runtime startup therefore cannot trigger registry access or compilation.

Copy `.env.example` to `.env` only when overriding ports, the test-wallet
build, or deterministic local identities. The checked-in defaults run without
an env file.

## Deterministic identities

- Node authority: `//Alice`
- Worker 0: `//Alice`
- Worker 1: `//Bob`
- Worker 2: `//Charlie`
- Playground relayer seed: byte `0x92` repeated 32 times

The Runtime contains only the relayer's derived public account. The Playground
reads the actual genesis hash from RPC at startup and exposes it through its
same-origin configuration endpoint, so browser action validation cannot drift
after a Runtime rebuild.

Every service has a dependency-aware health check. `up` waits for healthy
Node, Compiler, Playground, all three Workers, and Web. On E2E failure, `test`
prints Compose status plus recent logs for every relevant process.
