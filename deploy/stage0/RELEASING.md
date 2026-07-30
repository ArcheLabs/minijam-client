# Minimal Stage 0 release process

The `Stage 0 Release` workflow is the only publishing path. It runs on a
matching `v*-stage0*` tag, or by manual dispatch when `publish` is explicitly
enabled and a valid release tag is supplied. Ordinary pushes and pull requests
do not publish images.

Repository administrators must configure the Jambda GitHub App credentials and
the runtime smoke secrets listed in `deploy/stage0/secrets/README.md`. The
authority and Worker credentials must match the public identities in the
Stage 0 chain spec. GHCR package creation for the ArcheLabs organization must
allow public packages.

The single release gate:

1. checks Rust formatting, workspace tests, and Runtime Wasm;
2. tests the Compiler image and Playground Web production build;
3. runs the two existing M9 cross-process E2E flows;
4. builds the five production images from one MiniJAM commit and one pinned
   Jambda commit;
5. publishes immutable `stage0-<short-sha>` and release tags;
6. logs out of GHCR and verifies anonymous digest pulls;
7. exports the plain/raw chain specs, starts the raw spec, and records its
   genesis hash;
8. creates the release manifest and checksums;
9. starts `compose.stage0.yml` from the public digests and reuses the M9
   Playwright flow for Build, Deploy, Work, Accumulate, Upgrade, and restart
   recovery.

The resulting Actions Artifact contains:

- `minijam-node`;
- `minijam-worker`;
- `minijam_runtime.compact.compressed.wasm`;
- `stage0-plain.json`;
- `stage0-raw.json`;
- `release-manifest.json`;
- `SHA256SUMS`.

The manifest is the source of truth for the MiniJAM commit, Jambda commit,
genesis hash, public accounts, and all five image digests. A failed gate is not
a release. Do not retag or rebuild images after the gate; create a new release
from the corrected commit.

This minimal process has no Canary, deployment promotion, Kubernetes, or
mainnet approval stage.
