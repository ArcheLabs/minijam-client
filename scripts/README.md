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

## Stage-1

- `stage1-core-packages.sh`
- `test-stage1-core.sh`
- `test-stage1-docker-smoke.sh`
- `test-stage1-native-create-service.sh`
- `export-stage1-chain-specs-image.sh`

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
- `export-stage1-chain-specs-image.sh` generates specs from an exact node image;
  generated JSON is never committed.
- `test-stage1-native-create-service.sh` is an opt-in operator gate. It starts
  already-built native node and Formal RPC binaries, checks finalized-head
  progress, and exercises `minijam_createServiceV1` through its finalized
  receipt, preimage, dispatch, and finalized `ServiceInfo` checks. It never
  builds images or invokes Docker.

When adding a script, add its workflow or documentation caller in the same
change and keep the repository root resolution independent of the current
working directory.
