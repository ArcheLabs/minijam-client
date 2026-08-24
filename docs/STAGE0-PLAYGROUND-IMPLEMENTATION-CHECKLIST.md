# MiniJAM Stage 0 Playground Implementation Checklist

> Historical M2/M3 checklist. The current protocol path is ownerless System
> Service ABI V2; Controller/Upgrade items below are retained as archaeology
> only and are not production API semantics.

This checklist tracks the implementation against
[`minijam-stage0-playground-implementation-spec.md`](minijam-stage0-playground-implementation-spec.md).
An item is checked only when its implementation and automated acceptance evidence
exist in the repository.

## Baseline

- Review baseline: `5ba32647c1aa43425ca3a2d9aa11667c07f48c94`
- Jambda submodule baseline: `e15168173267ec1191dbf4d5aa48b2e4abe4cff9`
- Current decision: not releasable

## M0: Baseline and terminology

- [x] Canonical implementation specification is tracked under `docs/`.
- [x] Existing implementation facts are reconciled with the new specification.
- [x] Stage 0 architecture decisions and deviations are recorded.
- [x] `submit_work` is documented as user/relayer Work ingress.

Evidence:

- `docs/stage0-playground-architecture-decisions.md`
- `deploy/stage0/RELEASING.md`

## M1: Economics, ingress, and Worker permissions

- [x] Stage 0 economic prices, deposits, bonds, rewards, and slashes are zero.
- [x] Work, system-op, and preimage ingress accept only the configured relayer.
- [x] Candidate submitter is a registered, assigned Worker.
- [x] Pending tasks expose assignment and deterministic candidate producer.
- [x] Runtime, pallet, and Wasm acceptance commands pass.

Evidence:

- `cargo test -p pallet-minijam -p minijam-worker`
- `cargo test -p pallet-minijam-workers`
- `SKIP_WASM_BUILD=1 cargo test -p minijam-runtime`
- `cargo check -p minijam-runtime --no-default-features --target wasm32v1-none`

## M2: Controller and upgrade protocol

- [x] System commands carry the explicit user Controller.
- [x] Stable create, upgrade, and rejected receipts are implemented.
- [x] Controller queries are available through Runtime API and node RPC.
- [x] Owner upgrade and non-owner rejection are tested.
- [x] Upgrade preimage becomes ready without a Runtime panic.

## M3: Service 0 and SDK

- [x] Minimal C/C++ Service SDK is versioned.
- [x] Service 0 is reproducibly built from source as a real PVM program.
- [x] CreateService executes through the Service 0 VM path.
- [x] The Jambda adapter handles only the documented Stage 0 upgrade deviation.
- [x] Counter C and C++ services compile and execute.

Evidence:

- `./scripts/check-service-sdk.sh` (native plus C/C++ ELF → PolkaVM → JAM blob smoke)
- `./scripts/build-system-service.sh` produces the manifest-pinned Service 0 blob reproducibly.
- `cargo test -p minijam-runtime system_service`
- `cargo test -p minijam-runtime system_ops_execute_through_real_jambda_executor`
- `./scripts/build-counter-services.sh` reproduces the committed C/C++ artifacts.
- `./scripts/test-counter-services.sh` executes committed blobs through real
  Refine and Accumulate paths without invoking Clang.
- `service-toolchain/compiler/toolchain.lock` pins the release blob toolchain.

## M4: Finalized context and WorkPackage builder

- [x] Finalized context and historical `At` RPCs are implemented.
- [x] Pure Rust WorkPackage builder emits canonical package and Jambda bundle.
- [x] Bundle/CID golden tests pass.
- [x] Worker state reads are bound to the package lookup anchor.
- [x] Fixed allow-all authorization is deterministic and non-user-selectable.

## M5: Chain client and Compiler

- [x] Shared chain client owns signing, nonce, submission, finality, and events.
- [x] Compiler API builds fixed-toolchain C and restricted C++.
- [x] Compiler isolation and resource limits are tested.
- [x] Compiler output is deterministic and accepted by Jambda.

## M6: Minimal Playground orchestration API

- [x] Signed actions bind account, action, parameters, domain, genesis, and expiry,
      and are single-use.
- [x] SQLite persists only signed actions and operations.
- [x] Public build and signed create/upgrade/work routes are implemented.
- [x] Upgrade and Work authorization read the finalized on-chain Controller.
- [x] Create and Work operations recover after restart without duplicate chain
      submission.
- [x] Content-addressed Bundle files are validated and Worker-readable through
      `/ipfs/:cid`.
- [x] Liveness and readiness endpoints are implemented.

This minimal milestone intentionally excludes sessions, bearer tokens, a
standalone auth crate, build/job-event/bundle databases, garbage collection,
multi-instance leases, metrics, OpenAPI, WebSockets, full IPFS, and the
Playground Web UI.

## M7: Independent Worker verification and deployment

- [x] Only the assigned Candidate producer downloads, verifies, independently
      Refines, signs, and submits a Candidate.
- [x] Assigned validators independently repeat Refine against the lookup anchor
      before submitting Support or Oppose.
- [x] Bundle, historical-state, or VM failure cannot produce Support.
- [x] Non-assigned and already-submitted Worker identities are skipped using
      finalized chain tasks as the recovery source of truth.
- [x] Three distinct signing identities cover producer and validator execution.
- [x] A single non-root Worker image, seed-file configuration, independent data
      directories, and live/ready health endpoints are documented.

Complex Worker databases, dynamic Worker sets, multi-Core execution, economic
extensions, Kubernetes, full Compose, and the complete product E2E remain
outside this milestone. Full stack deployment belongs to M9.

## M8: Minimal Browser Playground

- [x] React, Vite, TypeScript, and locally bundled Monaco provide responsive
      Counter C/C++ editing and Compiler diagnostics.
- [x] An sr25519 extension adapter and fixed-key Playwright adapter connect
      wallets without sessions or bearer tokens.
- [x] Create, Upgrade, and Work each prepare, validate, display, and sign one
      parameter-bound action.
- [x] Rust and TypeScript consume shared canonical parameter-hash vectors.
- [x] Operation URLs recover polling after refresh and display only API states.
- [x] Service pages read finalized Controller, code, preimage, and Storage data
      exclusively through the Playground API.
- [x] Owner mismatch, replay, expiry, compiler, chain, Bundle, and Work errors
      have explicit user-facing messages.
- [x] Playwright covers Build, Deploy, Upgrade, Work, reload recovery, replay,
      authorization mismatch, and browser request isolation.
- [x] Responsive layout, labelled controls, textual errors, live status, and
      keyboard-dismissable signing confirmation provide the required baseline
      accessibility.

Sessions, account pages, Service discovery, multisig, browser compilation or
Refine, WorkPackage/Bundle construction, Worker dashboards, WebSockets, and
full-stack deployment remain outside this minimal milestone.

## M9: Local Stage 0 full stack and cross-process E2E

- [x] One Compose project starts Node, Compiler API, persistent Playground API,
      three independently keyed Workers, and Playground Web.
- [x] Dependency-aware readiness covers Node RPC, Compiler, Playground
      database/chain/compiler, Worker identity/RPC/Bundle gateway, and Web.
- [x] The browser reaches Node, Compiler, Workers, and Bundle storage only
      through same-origin Playground API routes.
- [x] Playwright proves Build, signed Create, finalized Preimage, Bundle fetch,
      Candidate, independent Vote, Accumulate, finalized Storage, and signed
      Upgrade across real processes.
- [x] Recovery E2E restarts a non-terminal Playground operation and one Worker,
      then verifies the same Work completes without duplicate recovery entries.
- [x] Owner mismatch returns 403 without changing the relayer nonce.
- [x] `build`, `up`, `down`, `reset`, and `test` are provided, with
      automatic Compose status and log capture on failure.

Multiple Nodes or Playgrounds, external databases, full IPFS, Kubernetes,
production TLS, monitoring stacks, load testing, and image publishing remain
outside this local integration milestone.

## M10: CI and release

- [ ] Required Rust, Web, Compiler, E2E, artifact, and container jobs are gated.
- [ ] Release artifacts, hashes, image digests, SBOM, and commits are recorded.
- [ ] Three consecutive clean CI runs pass.
- [ ] Upgrade and rollback rehearsal passes.
- [ ] 48-hour canary completes without unresolved critical incidents.
- [ ] All release gates are checked and `Decision: approved`.
