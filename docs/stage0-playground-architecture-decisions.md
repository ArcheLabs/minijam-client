# Stage 0 Playground Architecture Decisions

Status: accepted for Stage 0. These decisions clarify product and protocol
boundaries; deviations are temporary only where explicitly stated.

## Work ingress terminology

`pallet_minijam::submit_work` creates a Work request from a canonical
WorkPackage and Bundle reference. The Playground relayer calls it after
authenticating and authorizing the Service Controller. Worker output uses
`submit_candidate`; Worker assurance uses
`pallet_minijam_workers::submit_vote`.

Product-facing names are therefore:

- Create Work Request → `submit_work`
- Submit WorkReport Candidate → `submit_candidate`
- Submit Worker Vote → `submit_vote`

The pallet call is not renamed in Stage 0 so existing call indexes and clients
remain compatible.

## Trusted ingress relayer

The browser talks only to the Playground API. The API authenticates a
Substrate `AccountId32`, checks the finalized on-chain Controller, and submits
state-changing ingress extrinsics through a configured relayer. Runtime origin
filters restrict Work, system-op, and preimage ingress to that relayer, so API
authorization cannot be bypassed by calling the chain directly. Candidate and
vote calls remain restricted to Worker identities instead.

The relayer is an ingress mechanism, not the Service owner. Its key is supplied
as a deployment secret and is never committed.

## Explicit Controller

Create and upgrade commands carry the authenticated user's AccountId32
explicitly. Service 0 stores it at
`system/controller/<service_id>`. The system-op sender remains the relayer for
nonce and audit purposes. Playground authorization always reads the Controller
from finalized chain state; the local database is not authoritative.

## Service management execution

CreateService must execute through the real Service 0 PVM program and normal
Jambda accumulation. A native CreateService adapter is not an acceptable
release path.

Stage 0 permits one narrow deviation for UpgradeService because the standard
upgrade host call cannot update an arbitrary target Service from Service 0.
The isolated MiniJAM upgrade adapter must validate the on-chain Controller,
Service existence, code length, gas configuration, and preimage lookup
request. It may update only those fields and the stable upgrade receipt. This
deviation is to be replaced by a formal Service-management ABI after Stage 0.

## Default authorization

Stage 0 supports one fixed allow-all authorizer. WorkPackages still carry valid
authorization host, hash, configuration, and token fields; users cannot select
or alter the authorization mode. The preferred implementation is a real
genesis preimage executed by Jambda. A Worker-layer adapter is acceptable only
behind a fixed Stage 0 operator configuration, must not change generic Jambda
semantics, and must be documented in release evidence.

## Finalized execution context

Package construction, Worker Refine, Candidate validation, and state
observation use the same finalized anchor. Historical state reads are addressed
by the package lookup-anchor block hash. Advancing best state must not change a
report produced for an existing Work request.

