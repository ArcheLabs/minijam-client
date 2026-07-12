// Foundational types and bounded model constants for MiniJAM.
//
// Cryptography, SCALE bytes, canonical report bytes, CIDs and signatures are
// modelled as opaque integers. The model checks equality, ordering and state
// transitions, not byte-level encodings.
module types {
  type Option[a] = Some(a) | None

  type Hash = { hashBytes: int }
  type AccountId = MkAccountId(int) | _AccountIdSentinel
  type WorkerId = MkWorkerId(int) | _WorkerIdSentinel
  type WorkId = MkWorkId(int) | _WorkIdSentinel
  type ServiceId = MkServiceId(int) | _ServiceIdSentinel
  type Nonce = int
  type BlockNumber = int
  type EpochIndex = int
  type Round = int
  type Balance = int
  type Gas = int

  type Namespace =
    | System
    | ServiceInfo
    | ServiceStorage
    | ServiceLookup
    | Preimage
    | AdminBridge
    | ForbiddenNamespace

  type StateOperation = Upsert | Update | Remove

  type WorkStatus =
    | InsufficientWorkers
    | AwaitingCandidate
    | Voting
    | Accepted
    | Executed
    | Failed

  type Verdict = Support | Oppose
  type RoundDecision = RoundAccepted | RoundRejected

  type ExecutionOutcome =
    | ExecutionOk
    | OutOfGas
    | Trap
    | ServiceFailure
    | FatalStateError
    | FatalInvariantError

  type BulletinFault =
    | NoBulletinFault
    | Missing
    | Corrupt
    | Expired
    | Timeout

  pure val ProtocolVersion: int = 1
  pure val ChainId: Hash = { hashBytes: 77 }

  // Small finite-domain constants. These preserve MiniJAM semantics while
  // keeping Quint exploration tractable.
  pure val MaxWorkers: int = 5
  pure val TopWorkers: int = 4
  pure val WorkersPerWork: int = 3
  pure val SupportThreshold: int = 2
  pure val OpposeThreshold: int = 2
  pure val MaxCandidateRounds: int = 3
  pure val EpochLength: int = 4
  pure val VoteWindow: int = 2
  pure val ReportSubmissionDeadline: int = 2
  pure val EquivocationSuspension: int = 2
  pure val MinimumWorkerStake: Balance = 100
  pure val WorkDeposit: Balance = 10
  pure val CandidateBond: Balance = 10
  pure val TimelyVoteReward: Balance = 1
  pure val AcceptedSubmitterReward: Balance = 1
  pure val AbsenceSlash: Balance = 1
  pure val EquivocationSlash: Balance = 20
  pure val InitialRewardPool: Balance = 10000
  pure val MaxGas: Gas = 1000

  pure val WorkerDomain: Set[WorkerId] =
    1.to(MaxWorkers).map(i => MkWorkerId(i))

  pure val WorkDomain: Set[WorkId] =
    1.to(4).map(i => MkWorkId(i))

  pure val AccountDomain: Set[AccountId] =
    1.to(6).map(i => MkAccountId(i))

  pure def workerAccount(w: WorkerId): AccountId =
    match w {
      | MkWorkerId(i) => MkAccountId(i)
      | _WorkerIdSentinel => MkAccountId(0)
    }

  pure def hashOf(i: int): Hash = { hashBytes: i }

  pure def namespaceAllowed(ns: Namespace): bool =
    ns == System or
    ns == ServiceInfo or
    ns == ServiceStorage or
    ns == ServiceLookup or
    ns == Preimage or
    ns == AdminBridge

  pure def mapRemove(m: a -> b, k: a): a -> b =
    m.keys().exclude(Set(k)).mapBy(x => m.get(x))

  pure def listContains(xs: List[a], x: a): bool =
    xs.indices().exists(i => xs[i] == x)
}
