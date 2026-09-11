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
- `run-stage1-native-create-service-local.sh`
- `test-stage1-docker-create-service.sh`
- `test-stage1-native-work-e2e.sh`
- `test-stage1-native-local-lifecycle.sh`
- `test-stage1-native-work-recovery.sh`
- `test-stage1-docker-work-e2e.sh`
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
- `run-stage1-native-create-service-local.sh` performs the targeted incremental
  debug build, creates ephemeral test credentials, and runs the native gate.
  Set `MINIJAM_SKIP_BUILD=1` to reuse existing debug binaries after script-only
  changes.
- `test-stage1-docker-create-service.sh` runs the same CreateService request
  against exact Compose image references and preserves Compose logs and the
  JSON response when an artifact directory is provided.
- `stage1-native-local-up.sh` starts the persistent, isolated
  `stage1-work-e2e` network: one Alice consensus node, Formal RPC, and
  worker-0/1/2 using the local Alice/Bob/Charlie identities. It waits for
  node best/finality progress, Formal readiness, and a successful node RPC
  poll from every worker. The first start builds node, Formal RPC, and worker;
  set `MINIJAM_SKIP_BUILD=1` to reuse existing binaries.
- `stage1-native-local-status.sh` reports all five roles, block progression,
  finality, worker health, and successful worker node polls without exposing
  credentials or provider internals through `connection.env`.
- `stage1-native-local-down.sh` stops workers in reverse order, then Formal RPC
  and the node, retaining logs, chain spec, bundles, and per-worker recovery
  databases by default. Set `MINIJAM_LOCAL_PURGE=1` to remove the runtime
  directory.
- `test-stage1-native-local-lifecycle.sh` verifies READY, idempotent
  ALREADY_RUNNING, full status, clean shutdown, and a data-reusing restart.
- `test-stage1-native-work-e2e.sh` is a consumer-side gate: it never starts or
  stops the provider and submits a real counter Work package through Formal
  RPC, observing chain assignment, worker candidate refinement, support-vote
  acceptance, Imported, and finalized state.
- `test-stage1-native-work-recovery.sh` restarts worker-1, Formal RPC, and the
  node one at a time and runs the real Work gate after each recovery.
- `test-stage1-docker-work-e2e.sh` runs the same real Work acceptance path on
  `deploy/stage1/compose.work-e2e.yml`. The compact production Compose profile
  remains a one-worker Stage-1 deployment and is not changed by this profile.

When adding a script, add its workflow or documentation caller in the same
change and keep the repository root resolution independent of the current
working directory.
