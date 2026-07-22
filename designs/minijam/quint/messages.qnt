// Protocol messages and virtual JAM block execution records.
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

  type CanonicalWorkReport = {
    reportHash: Hash,
    packageHash: Hash,
    exportsRoot: Hash,
    authorizerHash: Hash,
    core: int,
    accumulateGas: Gas,
    dependencies: Set[Hash]
  }

  type CanonicalPreimage = {
    preimageHash: Hash,
    requester: ServiceId,
    blobLength: int
  }

  type ReportEnvelope = {
    protocolVersion: int,
    chainId: Hash,
    workId: WorkId,
    service: ServiceId,
    round: Round,
    reportHash: Hash,
    metadata: ReportMetadata,
    canonical: CanonicalWorkReport,
    bulletinFault: BulletinFault
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
    protocolVersion: int,
    slot: BlockNumber,
    parentHash: Hash,
    parentStateRoot: Hash,
    entropy: Hash,
    reports: List[CanonicalWorkReport],
    preimages: List[CanonicalPreimage],
    maxGas: Gas
  }

  type ExecutionOutput = {
    importedReports: Set[Hash],
    importedPreimages: Set[Hash],
    headerHash: Hash,
    accumulateRoot: Hash,
    gasUsed: Gas,
    receiptHash: Hash,
    outcome: ExecutionOutcome,
    phases: List[ExecutivePhase],
    ticketsUsed: Set[Hash],
    removedStfs: Set[RemovedStf]
  }

  pure def reportGas(reports: List[CanonicalWorkReport]): Gas =
    reports.indices().fold(0, (acc, i) => acc + reports[i].accumulateGas)

  pure def reportHashes(reports: List[CanonicalWorkReport]): Set[Hash] =
    reports.indices().map(i => reports[i].reportHash)

  pure def preimageHashes(preimages: List[CanonicalPreimage]): Set[Hash] =
    preimages.indices().map(i => preimages[i].preimageHash)

  pure def virtualHeaderHash(input: ExecutionInput): Hash =
    { hashBytes:
        input.parentHash.hashBytes +
        input.parentStateRoot.hashBytes +
        input.entropy.hashBytes +
        input.slot +
        input.reports.length() * 17 +
        input.preimages.length() * 19 +
        input.protocolVersion
    }

  pure def virtualReceiptHash(input: ExecutionInput, output: ExecutionOutput): Hash =
    { hashBytes:
        output.headerHash.hashBytes +
        output.accumulateRoot.hashBytes +
        output.gasUsed +
        output.importedReports.size() * 23 +
        output.importedPreimages.size() * 29 +
        input.maxGas
    }
}
