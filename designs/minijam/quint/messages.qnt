// Protocol messages and execution records for the MiniJAM model.
module messages {
  import types.* from "./types"

  type WorkPackage = {
    workId: WorkId,
    owner: AccountId,
    service: ServiceId,
    payloadHash: Hash,
    contextHash: Hash,
    parentStateRoot: Hash,
    serviceCodeHash: Hash,
    gasLimit: Gas,
    importsRoot: Hash
  }

  type ReportMetadata = {
    packageHash: Hash,
    contextHash: Hash,
    exportsRoot: Hash,
    accumulateGas: Gas
  }

  type ProtocolKey = {
    namespace: Namespace,
    local: int
  }

  type StateChange = {
    key: ProtocolKey,
    op: StateOperation,
    value: Option[int]
  }

  type ServiceOutput = {
    service: ServiceId,
    outputHash: Hash
  }

  type BridgeDirection = BridgeInbound | BridgeOutbound

  type BridgeEffect = {
    direction: BridgeDirection,
    nonce: Nonce,
    service: ServiceId,
    account: AccountId,
    amount: Balance
  }

  type ReportEnvelope = {
    protocolVersion: int,
    chainId: Hash,
    workId: WorkId,
    service: ServiceId,
    round: Round,
    reportHash: Hash,
    metadata: ReportMetadata,
    parentStateRoot: Hash,
    postStateRoot: Hash,
    bulletinFault: BulletinFault,
    delta: List[StateChange],
    outputs: List[ServiceOutput],
    bridgeEffects: List[BridgeEffect],
    gasUsed: Gas,
    outcome: ExecutionOutcome
  }

  type WorkerVote = {
    worker: WorkerId,
    workId: WorkId,
    round: Round,
    candidateHash: Hash,
    verdict: Verdict,
    deadline: BlockNumber,
    chainId: Hash,
    protocolVersion: int
  }

  type ExecutionInput = {
    slot: BlockNumber,
    epoch: EpochIndex,
    reports: List[ReportEnvelope],
    maxGas: Gas,
    protocolVersion: int
  }

  type ExecutionOutput = {
    orderedChanges: List[StateChange],
    consumedReports: List[Hash],
    serviceOutputs: List[ServiceOutput],
    bridgeEffects: List[BridgeEffect],
    gasUsed: Gas,
    receiptHash: Hash,
    outcome: ExecutionOutcome
  }

  pure def keyLess(a: ProtocolKey, b: ProtocolKey): bool =
    if (a.namespace == b.namespace) a.local < b.local
    else namespaceRank(a.namespace) < namespaceRank(b.namespace)

  pure def namespaceRank(ns: Namespace): int =
    if (ns == System) 0
    else if (ns == ServiceInfo) 1
    else if (ns == ServiceStorage) 2
    else if (ns == ServiceLookup) 3
    else if (ns == Preimage) 4
    else if (ns == AdminBridge) 5
    else 99

  pure def changesOrderedUnique(changes: List[StateChange]): bool =
    changes.indices().forall(i =>
      i == 0 or keyLess(changes[i - 1].key, changes[i].key))

  pure def changesAllowed(changes: List[StateChange]): bool =
    changes.indices().forall(i => namespaceAllowed(changes[i].key.namespace))

  pure def stateRootAfter(parent: Hash, reportHash: Hash): Hash =
    { hashBytes: parent.hashBytes + reportHash.hashBytes + 1 }
}
