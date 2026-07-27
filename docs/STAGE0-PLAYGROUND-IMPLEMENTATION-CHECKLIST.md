# MiniJAM Stage 0 Playground Implementation Checklist

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
- `docs/stage0-public-testnet-checklist.md`
- `docs/STAGE0-RELEASE-CHECKLIST.md`

## M1: Economics, ingress, and Worker permissions

- [x] Stage 0 economic prices, deposits, bonds, rewards, and slashes are zero.
- [x] Work, system-op, and preimage ingress accept only the configured relayer.
- [x] Candidate submitter is a registered, assigned Worker.
- [x] Pending tasks expose assignment and deterministic candidate producer.
- [x] Runtime, pallet, and Wasm acceptance commands pass.

Evidence:

- `cargo test -p pallet-minijam -p minijam-worker`
- `SKIP_WASM_BUILD=1 cargo test -p minijam-runtime`
- `cargo check -p minijam-runtime --no-default-features --target wasm32v1-none`

## M2: Controller and upgrade protocol

- [ ] System commands carry the explicit user Controller.
- [ ] Stable create, upgrade, and rejected receipts are implemented.
- [ ] Controller queries are available through Runtime API and node RPC.
- [ ] Owner upgrade and non-owner rejection are tested.
- [ ] Upgrade preimage becomes ready without a Runtime panic.

## M3: Service 0 and SDK

- [ ] Minimal C/C++ Service SDK is versioned.
- [ ] Service 0 is reproducibly built from source as a real PVM program.
- [ ] CreateService executes through the Service 0 VM path.
- [ ] The Jambda adapter handles only the documented Stage 0 upgrade deviation.
- [ ] Counter C and C++ services compile and execute.

## M4: Finalized context and WorkPackage builder

- [ ] Finalized context and historical `At` RPCs are implemented.
- [ ] Pure Rust WorkPackage builder emits canonical package and Jambda bundle.
- [ ] Bundle/CID golden tests pass.
- [ ] Worker state reads are bound to the package lookup anchor.
- [ ] Fixed allow-all authorization is deterministic and non-user-selectable.

## M5: Chain client and Compiler

- [ ] Shared chain client owns signing, nonce, submission, finality, and events.
- [ ] Compiler API builds fixed-toolchain C and restricted C++.
- [ ] Compiler isolation and resource limits are tested.
- [ ] Compiler output is deterministic and accepted by Jambda.

## M6: Auth, orchestration, and Bundle gateway

- [ ] MultiSignature challenge authentication prevents expiry and replay attacks.
- [ ] Opaque sessions and SQLite migrations are implemented.
- [ ] Public build and protected create/upgrade/work routes are implemented.
- [ ] Authorization reads the finalized on-chain Controller.
- [ ] Jobs recover after API restart without duplicate Work.
- [ ] Content-addressed Bundle gateway is safe and Worker-compatible.

## M7: Worker and deployment hardening

- [ ] Candidate producer selection avoids multi-Worker races.
- [ ] Votes independently rerun Refine and can Support or Oppose.
- [ ] Worker key files and Stage 0 metrics are implemented.
- [ ] Three configured Workers complete the Candidate/Vote lifecycle.

## M8: Browser Playground

- [ ] React/Vite/Monaco single-page Playground is implemented.
- [ ] Wallet login, Build, Deploy, Upgrade, Work, and state views are functional.
- [ ] The browser never calls node RPC directly or fabricates job state.
- [ ] Accessibility, CSP, CORS, and error categories are tested.
- [ ] Playwright covers the complete browser flow.

## M9: Docker and cross-process E2E

- [ ] Development and Stage 0 Compose stacks are operational.
- [ ] Health and readiness checks cover all required dependencies.
- [ ] All 24 cross-process scenarios have executable evidence.
- [ ] Local Docker completes Build → Deploy → Work → Accumulate → Upgrade.

## M10: CI and release

- [ ] Required Rust, Web, Compiler, E2E, artifact, and container jobs are gated.
- [ ] Release artifacts, hashes, image digests, SBOM, and commits are recorded.
- [ ] Three consecutive clean CI runs pass.
- [ ] Upgrade and rollback rehearsal passes.
- [ ] 48-hour canary completes without unresolved critical incidents.
- [ ] All release gates are checked and `Decision: approved`.
