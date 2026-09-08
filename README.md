# MiniJAM Client

MiniJAM Client is the Stage-1 node, worker, runtime, protocol, and optional
Service toolchain implementation. Stage-1 is the supported deployment line;
historical developer products and generated network artifacts are not part of
the repository surface.

English | [简体中文](README.zh-CN.md)

See [MiniJamSpec](docs/minijam-spec.md), the
[execution boundary](docs/execution-boundary.md), and the
[compatibility matrix](docs/compatibility-matrix.md).

## Repository layout

| Path | Responsibility |
| --- | --- |
| `crates/minijam-protocol` | Protocol constants, content references, reports, votes, and state changes |
| `crates/minijam-jamcore-api` | Versioned JamCore interface and execution types |
| `crates/minijam-worker-engine` | Runtime-independent Worker algorithms |
| `crates/minijam-worker` | Worker daemon and bundle fetching |
| `crates/minijam-formal-rpc` | Application-neutral Work and bundle gateway |
| `runtime` | FRAME Runtime and jambda Executive integration |
| `node` | Node CLI, RPC, chain profiles, Aura, and GRANDPA |
| `deploy/stage1` | Canonical Stage-1 Dockerfile and Compose profiles |
| `deploy/compiler` | Optional Service compiler image |
| `service-toolchain` | Service 0 compiler and conformance sources |
| `scripts` | CI, Stage-1, release, and Service toolchain checks |

## Stage-1 deployment

The production unit is the exact image digest and the matching generated chain
specification. The canonical images are:

```text
ghcr.io/archelabs/minijam-node
ghcr.io/archelabs/minijam-worker
ghcr.io/archelabs/minijam-formal-rpc
```

Use `deploy/stage1/compose.compact.yml` for one-host deployment and
`deploy/stage1/compose.split.yml` when the private chain network spans hosts.
Generate `stage1.json` and `stage1-raw.json` from the exact node image with
`scripts/export-stage1-chain-specs-image.sh`; generated specs must not be
committed.

Formal RPC owns the Work-ingress relayer and bundle store. The Worker owns its
signing key. Node RPC, Worker health, metrics, and compiler endpoints remain
private deployment concerns.

## Development

The pinned Rust toolchain installs `rustfmt`, `clippy`, `rust-src`,
`wasm32-unknown-unknown`, and `wasm32v1-none`. Full Runtime and node builds
require the pinned private `external/jambda` submodule.

```bash
git submodule update --init external/jambda
cargo fmt --all -- --check
cargo test --workspace --exclude minijam-runtime --exclude minijam-node
```

Core Wasm checks:

```bash
cargo check -p minijam-protocol -p minijam-jamcore-api \
  -p minijam-worker-engine --no-default-features \
  --target wasm32-unknown-unknown
cargo check -p minijam-runtime --no-default-features --target wasm32v1-none
```

Heavy builds and Docker smoke tests belong in GitHub Actions. The repository
does not require a host-built web product or local release stack.

## Protocol

MiniJAM uses an independent Polkadot SDK chain with a deliberately bounded
JAM-compatible execution surface. Work reports, Worker votes, state changes,
Service 0 execution, bridge effects, and bundle retrieval are application-
neutral protocol concerns. See the documents under `docs/` for the execution,
toolchain, and deployment boundaries.

## License

Apache License 2.0.
