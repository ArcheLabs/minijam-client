// Top-level MiniJAM model state.
module state {
  import types.* from "./types"
  import messages.* from "./messages"
  import jam_projection.* from "./jam_projection"

  type WorkerRecord = {
    owner: AccountId,
    activeStake: Balance,
    effectiveEpoch: EpochIndex,
    suspendedUntil: EpochIndex,
    active: bool
  }

  type WorkRecord = {
    owner: AccountId,
    service: ServiceId,
    package: WorkPackage,
    round: Round,
    status: WorkStatus,
    candidateDeadline: BlockNumber,
    executeAt: BlockNumber
  }

  type CandidateRecord = {
    submitter: AccountId,
    envelope: ReportEnvelope,
    voteDeadline: BlockNumber
  }

  type VotingRound = {
    candidateHash: Hash,
    deadline: BlockNumber,
    support: int,
    oppose: int,
    responses: Set[WorkerId],
    locked: Option[RoundDecision]
  }

  type BridgeDirection = BridgeInbound | BridgeOutbound

  type BridgeEffect = {
    direction: BridgeDirection,
    nonce: Nonce,
    service: ServiceId,
    account: AccountId,
    amount: Balance
  }

  type BridgeLedger = {
    nextInboundNonce: Nonce,
    escrow: AccountId -> Balance,
    consumedOutbound: Set[Nonce],
    observedStateKeys: Set[Hash]
  }

  type BulletinEntry = {
    contentHash: Hash,
    retentionUntil: BlockNumber,
    fault: BulletinFault
  }

  type BulletinState = {
    entries: Hash -> BulletinEntry,
    authorized: Set[AccountId],
    blockNumber: BlockNumber
  }

  type MiniJamState = {
    workers: WorkerId -> WorkerRecord,
    activeWorkers: Set[WorkerId],
    assignments: (WorkId, Round) -> Set[WorkerId],
    works: WorkId -> WorkRecord,
    candidates: (WorkId, Round) -> CandidateRecord,
    votes: (WorkId, Round, WorkerId) -> WorkerVote,
    equivocations: Set[(WorkId, Round, WorkerId)],
    executionQueue: List[WorkId],
    quarantinedExecutionQueue: List[WorkId],
    executionPaused: bool,
    pendingPreimages: List[CanonicalPreimage],
    acceptedReports: Hash -> ReportEnvelope,
    jam: JamProjection,
    bridge: BridgeLedger,
    bulletin: BulletinState,
    rewardPool: Balance,
    totalExternalBalance: Balance,
    currentEpoch: EpochIndex
  }

  pure val emptyBridge: BridgeLedger = {
    nextInboundNonce: 0,
    escrow: Map(),
    consumedOutbound: Set(),
    observedStateKeys: Set()
  }

  pure val emptyBulletin: BulletinState = {
    entries: Map(),
    authorized: Set(),
    blockNumber: 0
  }

  pure val emptyMiniJam: MiniJamState = {
    workers: Map(),
    activeWorkers: Set(),
    assignments: Map(),
    works: Map(),
    candidates: Map(),
    votes: Map(),
    equivocations: Set(),
    executionQueue: List(),
    quarantinedExecutionQueue: List(),
    executionPaused: false,
    pendingPreimages: List(),
    acceptedReports: Map(),
    jam: emptyJamProjection,
    bridge: emptyBridge,
    bulletin: emptyBulletin,
    rewardPool: InitialRewardPool,
    totalExternalBalance: 0,
    currentEpoch: 0
  }

  pure def isActiveWorker(s: MiniJamState, w: WorkerId, epoch: EpochIndex): bool =
    s.workers.keys().contains(w) and
    s.activeWorkers.contains(w) and
    s.workers.get(w).active and
    s.workers.get(w).activeStake >= MinimumWorkerStake and
    s.workers.get(w).suspendedUntil <= epoch

  pure def assigned(s: MiniJamState, workId: WorkId, round: Round): Set[WorkerId] =
    if (s.assignments.keys().contains((workId, round)))
      s.assignments.get((workId, round))
    else Set()

  pure def hasWork(s: MiniJamState, workId: WorkId): bool =
    s.works.keys().contains(workId)

  pure def hasCandidate(s: MiniJamState, workId: WorkId, round: Round): bool =
    s.candidates.keys().contains((workId, round))

  pure def serviceRoot(s: MiniJamState, serviceId: ServiceId): Hash =
    hashOf(match serviceId { | MkServiceId(i) => i | _ServiceIdSentinel => 0 })

  pure def serviceCodeHash(s: MiniJamState, serviceId: ServiceId): Hash =
    hashOf(match serviceId { | MkServiceId(i) => 100 + i | _ServiceIdSentinel => 0 })
}
