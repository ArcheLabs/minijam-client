# Repository scripts

Scripts are kept at this level so workflow paths remain explicit and stable.
Each script has one caller or one documented operator purpose; retired product
and release helpers are removed instead of being kept as compatibility shims.

## CI and boundaries

- `check-minijam-boundary.sh`
- `check-production-image-boundary.sh`
- `check-release-secret-hygiene.sh`
- `check-stage1-boundary.sh`
- `check-stage1-core-boundary.sh`
- `check-stage1-core-dependencies.sh`
- `check-submodule.sh`

## Canonical Stage-1 local and testnet

- `stage1-core-packages.sh`
- `test-stage1-core.sh`
- `test-minijam-local-e2e.sh`
- `test-minijam-work-e2e.sh`
- `export-testnet-chain-specs-image.sh`

## Service toolchain

- `build-system-service.sh`
- `compile-service`
- `check-service-sdk.sh`
- `print-service-toolchain-diagnostics.sh`
- `test-compiler-image.sh`
- `test-counter-services.sh`
- `test-service0-provenance.sh`

## Release and maintenance

- `check-release-secret-hygiene.sh` validates release metadata before upload.
- `export-testnet-chain-specs-image.sh` generates deterministic `testnet.json`
  and `testnet-raw.json` from an exact node image; generated JSON is never
  committed.
- `test-minijam-local-e2e.sh` consumes one aggregate `minijam` image and
  starts the canonical local network through `minijam --dev`. It verifies node,
  Formal RPC, Worker 0, block/finality progress, Service creation, and a real
  Work through Imported and finalized state.
- `test-minijam-work-e2e.sh` is the provider-independent Work assertion used by
  the aggregate local gate. It does not create a second chain or start a
  second worker.

When adding a script, add its workflow or documentation caller in the same
change and keep the repository root resolution independent of the current
working directory.
