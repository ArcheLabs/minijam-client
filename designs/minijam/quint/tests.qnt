// Deterministic smoke tests for the MiniJAM Quint model.
module tests {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
  import workers.* from "./workers"
  import work.* from "./work"
  import refine.* from "./refine"
  import executive.* from "./executive"
  import bridge.* from "./bridge"
  import bulletin.* from "./bulletin"
  import main.* from "./main"

  pure def workPackage(id: WorkId): WorkPackage =
    defaultWorkPackage(id, MkAccountId(5), MkServiceId(1), serviceRoot(initialMiniJam, MkServiceId(1)), serviceCodeHash(initialMiniJam, MkServiceId(1)))

  pure val preparedWork: MiniJamState =
    submitCandidate(
      submitWork(initialMiniJam, workPackage(MkWorkId(1)), 0),
      MkWorkId(1),
      candidateReport(workPackage(MkWorkId(1)), 0, MkAccountId(6), Set(), 10),
      MkAccountId(6),
      0
    )

  pure def acceptedState(s: MiniJamState): MiniJamState = {
    val cand = s.candidates.get((MkWorkId(1), 0))
    val v1: WorkerVote = {
      worker: MkWorkerId(1),
      workId: MkWorkId(1),
      round: 0,
      candidateHash: cand.envelope.reportHash,
      verdict: Support,
      deadline: cand.voteDeadline,
      chainId: ChainId,
      protocolVersion: ProtocolVersion
    }
    castVote(castVote(s, v1, 0), { ...v1, worker: MkWorkerId(2) }, 0)
  }

  pure def baseInput(reports: List[CanonicalWorkReport], preimages: List[CanonicalPreimage], slot: int): ExecutionInput = {
    protocolVersion: ProtocolVersion,
    slot: slot,
    parentHash: hashOf(10),
    parentStateRoot: hashOf(20),
    entropy: hashOf(30),
    reports: reports,
    preimages: preimages,
    maxGas: MaxGas
  }

  run emptyVirtualBlockTest = {
    val result = executeVirtualJamBlock(initialMiniJam, baseInput(List(), List(), 1))
    all {
      assert(result._2.outcome == ExecutionOk),
      assert(result._2.phases == RetainedPhaseOrder),
      assert(result._1.jam.slot == 1),
      init
    }
  }

  run safroleReceivesEmptyTicketsTest = {
    val result = executeVirtualJamBlock(initialMiniJam, baseInput(List(), List(), 1))
    all {
      assert(result._2.ticketsUsed == Set()),
      assert(result._1.jam.safroleTransitions == 1),
      init
    }
  }

  run acceptedReportIsImportedAndProjectedTest = {
    val s0 = acceptedState(preparedWork)
    val cand = s0.candidates.get((MkWorkId(1), 0))
    val input = baseInput(List(cand.envelope.canonical), List(), 1)
    val result = executeVirtualJamBlock(s0, input)
    all {
      assert(result._2.importedReports == Set(cand.envelope.reportHash)),
      assert(result._1.jam.accumulateInputReports.contains(cand.envelope.reportHash)),
      assert(result._1.jam.currentHistoryPackages.contains(cand.envelope.metadata.packageHash)),
      assert(result._1.jam.authorizationConsumed.contains(cand.envelope.canonical.authorizerHash)),
      init
    }
  }

  run importedReportMayRemainReadyTest = {
    val pkg = workPackage(MkWorkId(1))
    val report = candidateReport(pkg, 0, MkAccountId(6), Set(hashOf(999)), 10)
    val input = baseInput(List(report.canonical), List(), 1)
    val result = executeVirtualJamBlock(initialMiniJam, input)
    all {
      assert(result._2.importedReports == Set(report.reportHash)),
      assert(result._1.jam.readyReports.contains(report.reportHash)),
      assert(not(result._1.jam.accumulatedReports.contains(report.reportHash))),
      init
    }
  }

  run preimagePrepareAndApplyTest = {
    val preimage: CanonicalPreimage = {
      preimageHash: hashOf(501),
      requester: MkServiceId(1),
      blobLength: 3
    }
    val result = executeVirtualJamBlock(initialMiniJam, baseInput(List(), List(preimage), 1))
    all {
      assert(result._2.importedPreimages == Set(preimage.preimageHash)),
      assert(result._1.jam.appliedPreimages.contains(preimage.preimageHash)),
      init
    }
  }

  run invalidSlotRollsBackTest = {
    val s0 = { ...initialMiniJam, jam: { ...initialMiniJam.jam, slot: 2 } }
    val result = executeVirtualJamBlock(s0, baseInput(List(), List(), 2))
    all {
      assert(result._2.outcome == FatalInvariantError),
      assert(result._1 == s0),
      init
    }
  }

  run overGasBatchRollsBackTest = {
    val report = canonicalReport(workPackage(MkWorkId(1)), 0, Set(), MaxGas + 1)
    val result = executeVirtualJamBlock(initialMiniJam, baseInput(List(report), List(), 1))
    all {
      assert(result._2.outcome == OutOfGas),
      assert(result._1 == initialMiniJam),
      init
    }
  }

  run removedStfsAreUnavailableTest = {
    val result = executeVirtualJamBlock(initialMiniJam, baseInput(List(), List(), 1))
    all {
      assert(result._2.removedStfs == Set(Disputes, Assurances, ReportsGuarantees)),
      init
    }
  }

  run receiptCommitsToInputAndOutputTest = {
    val input = baseInput(List(), List(), 1)
    val result = executeVirtualJamBlock(initialMiniJam, input)
    all {
      assert(result._2.receiptHash == virtualReceiptHash(input, result._2)),
      init
    }
  }

  run malformedVoteRejectedTest = {
    val s0 = preparedWork
    val cand = s0.candidates.get((MkWorkId(1), 0))
    val good: WorkerVote = {
      worker: MkWorkerId(1),
      workId: MkWorkId(1),
      round: 0,
      candidateHash: cand.envelope.reportHash,
      verdict: Support,
      deadline: cand.voteDeadline,
      chainId: ChainId,
      protocolVersion: ProtocolVersion
    }
    all {
      assert(castVote(s0, { ...good, chainId: hashOf(999) }, 0) == s0),
      assert(castVote(s0, { ...good, protocolVersion: ProtocolVersion + 1 }, 0) == s0),
      assert(castVote(s0, { ...good, candidateHash: hashOf(999) }, 0) == s0),
      assert(castVote(s0, { ...good, worker: MkWorkerId(5) }, 0) == s0),
      assert(castVote(s0, { ...good, deadline: -1 }, 0) == s0),
      init
    }
  }

  run bridgeInboundLedgerTest = {
    val s0 = bridgeIn(initialMiniJam, MkAccountId(1), MkServiceId(1), 5)
    all {
      assert(s0.bridge.nextInboundNonce == 1),
      assert(s0.bridge.escrow.get(MkAccountId(1)) == 5),
      init
    }
  }

  run bulletinFaultTest = {
    val content = hashOf(42)
    val s0 = storeContent(authorize(initialMiniJam, MkAccountId(1)), MkAccountId(1), content, 3)
    val s1 = injectFault(s0, content, Timeout)
    all {
      assert(fetchStatus(s1, content) == Timeout),
      assert(s1.jam == initialMiniJam.jam),
      init
    }
  }
}
