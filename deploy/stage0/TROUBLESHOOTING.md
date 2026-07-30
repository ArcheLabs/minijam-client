# Stage 0 troubleshooting

Use the exact manifest, raw chain spec, and digest image references from one
release. Start with:

```bash
docker compose \
  --env-file deploy/stage0/.env \
  -f compose.stage0.yml \
  ps
```

## Image pull fails

Confirm the image value includes `@sha256:` and contains no placeholder. The
release gate verifies anonymous pulls; an authentication prompt usually means
the package is not public or the digest was copied incorrectly.

## A service is unhealthy

Inspect only the relevant service:

```bash
docker compose \
  --env-file deploy/stage0/.env \
  -f compose.stage0.yml \
  logs node playground-api worker-1
```

Node readiness requires its database and raw chain spec. Playground readiness
requires Node and Compiler. Worker readiness additionally requires a signing
key whose public key equals its registered session key.

## Node RPC is unreachable

Node RPC is intentionally private. Other containers use `ws://node:9944` or
`http://node:9944`; the browser must not connect to it directly. Use `docker
compose exec node` for operator diagnostics rather than publishing the port.

## Compiler is unreachable

Compiler is intentionally private and has a read-only root filesystem,
temporary scratch space, resource limits, and no published port. Confirm
`compiler-api` is healthy and Playground uses
`http://compiler-api:8081`.

## Genesis hash mismatch

Do not override the hash manually. Copy `MINIJAM_GENESIS_HASH` from the same
`release-manifest.json` that supplied the image digests and raw chain spec.
Reset all volumes before changing releases.

## Relayer or Worker key is missing

An empty relayer value prevents Playground from starting. Worker seed files
must exist, be readable by Docker, and contain the URI matching that Worker's
registered public session key. Follow `deploy/stage0/secrets/README.md`; never
paste a credential into Compose, source control, a support ticket, or logs.

## The page opens but API calls fail

Check `playground-api` health, then request `/health` through the Web port.
The browser should use only the same-origin `/api/` and `/ipfs/` routes. It
must not access Node, Compiler, or Worker endpoints.

Stage 0 is resettable, Sudo-enabled, and has no real economic value. State and
APIs may change without permanence guarantees.
