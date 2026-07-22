// Abstract retained-STF MiniJAM Executive.
module executive {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
  import jam_projection.* from "./jam_projection"

  pure val RetainedPhaseOrder: List[ExecutivePhase] =
    List(
      Start,
      LastHistoryDone,
      SafroleDone,
      PreimagesPrepared,
      AccumulateDone,
      CurrentHistoryDone,
      AuthorizationsDone,
      StatisticsDone,
      PreimagesApplied,
      Finalized
    )

  pure val RemovedStfs: Set[RemovedStf] =
    Set(Disputes, Assurances, ReportsGuarantees)

  pure def emptyExecution: ExecutionOutput = {
    importedReports: Set(),
    importedPreimages: Set(),
    headerHash: hashOf(0),
    accumulateRoot: hashOf(0),
    gasUsed: 0,
    receiptHash: hashOf(0),
    outcome: FatalInvariantError,
    phases: List(),
    ticketsUsed: Set(),
    removedStfs: RemovedStfs
  }

  pure def validInput(jam: JamProjection, input: ExecutionInput): bool =
    input.protocolVersion == ProtocolVersion and
    input.slot > jam.slot and
    reportGas(input.reports) <= input.maxGas

  pure def executionOutput(input: ExecutionInput, outcome: ExecutionOutcome): ExecutionOutput = {
    val headerHash = virtualHeaderHash(input)
    val output0 = {
      importedReports: if (outcome == ExecutionOk) reportHashes(input.reports) else Set(),
      importedPreimages: if (outcome == ExecutionOk) preimageHashes(input.preimages) else Set(),
      headerHash: if (outcome == ExecutionOk) headerHash else hashOf(0),
      accumulateRoot: if (outcome == ExecutionOk) hashOf(headerHash.hashBytes + 1) else hashOf(0),
      gasUsed: if (outcome == ExecutionOk) reportGas(input.reports) else 0,
      receiptHash: hashOf(0),
      outcome: outcome,
      phases: if (outcome == ExecutionOk) RetainedPhaseOrder else List(Start),
      ticketsUsed: Set(),
      removedStfs: RemovedStfs
    }
    { ...output0, receiptHash: virtualReceiptHash(input, output0) }
  }

  pure def executeVirtualJamBlock(
    s: MiniJamState,
    input: ExecutionInput
  ): (MiniJamState, ExecutionOutput) = {
    if (not(validInput(s.jam, input))) {
      (s, executionOutput(input, if (reportGas(input.reports) > input.maxGas) OutOfGas else FatalInvariantError))
    } else {
      val headerHash = virtualHeaderHash(input)
      val accumulateRoot = hashOf(headerHash.hashBytes + 1)
      val j1 = progressLastHistory(s.jam, input.parentStateRoot)
      val j2 = progressSafrole(j1, input.slot, input.entropy, Set())
      val j3 = preparePreimages(j2, input.preimages)
      val j4 = progressAccumulate(j3, input.reports)
      val j5 = progressCurrentHistory(j4, headerHash, accumulateRoot, input.reports)
      val j6 = progressAuthorizations(j5, input.reports)
      val j7 = progressStatistics(j6, input)
      val j8 = applyPreimages(j7, input.preimages)
      val output = executionOutput(input, ExecutionOk)
      ({ ...s, jam: j8 }, output)
    }
  }

  pure def queuePreimage(s: MiniJamState, preimage: CanonicalPreimage): MiniJamState =
    if (s.pendingPreimages.indices().exists(i =>
        s.pendingPreimages[i].requester == preimage.requester and
        s.pendingPreimages[i].preimageHash == preimage.preimageHash and
        s.pendingPreimages[i].blobLength == preimage.blobLength))
      s
    else { ...s, pendingPreimages: s.pendingPreimages.append(preimage) }
}
