// Deterministic smoke tests for the MiniJAM Quint model.
module tests {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
  import workers.* from "./workers"
  import work.* from "./work"
  import refine.* from "./refine"
  import accumulate.* from "./accumulate"
  import bridge.* from "./bridge"
  import bulletin.* from "./bulletin"
  import main.* from "./main"

  pure val preparedWork: MiniJamState =
    submitCandidate(
      submitWork(
        initialMiniJam,
        defaultWorkPackage(MkWorkId(1), MkAccountId(5), MkServiceId(1), serviceRoot(initialMiniJam, MkServiceId(1)), serviceCodeHash(initialMiniJam, MkServiceId(1))),
        0
      ),
      MkWorkId(1),
      candidateReport(
        defaultWorkPackage(MkWorkId(1), MkAccountId(5), MkServiceId(1), serviceRoot(initialMiniJam, MkServiceId(1)), serviceCodeHash(initialMiniJam, MkServiceId(1))),
        0,
        MkAccountId(6),
        List(validServiceDelta(1, 10)),
        ExecutionOk
      ),
      MkAccountId(6),
      0
    )

  run happyPathTest = {
    val s0 = preparedWork
    val cand = s0.candidates.get((MkWorkId(1), 0))
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
    val v2 = { ...v1, worker: MkWorkerId(2) }
    val s1 = castVote(castVote(s0, v1, 0), v2, 0)
    val result = executeReport(s1, cand.envelope)
    all {
      assert(s1.works.get(MkWorkId(1)).status == Accepted),
      assert(result._2.outcome == ExecutionOk),
      assert(result._1.serviceStorage.get((MkServiceId(1), 1)) == 10),
      init
    }
  }

  run rejectPathTest = {
    val s0 = preparedWork
    val cand = s0.candidates.get((MkWorkId(1), 0))
    val baseVote: WorkerVote = {
      worker: MkWorkerId(1),
      workId: MkWorkId(1),
      round: 0,
      candidateHash: cand.envelope.reportHash,
      verdict: Oppose,
      deadline: cand.voteDeadline,
      chainId: ChainId,
      protocolVersion: ProtocolVersion
    }
    val s1 = castVote(castVote(s0, baseVote, 0), { ...baseVote, worker: MkWorkerId(2) }, 0)
    all {
      assert(s1.works.get(MkWorkId(1)).status == Failed),
      init
    }
  }

  run insufficientWorkersTest = {
    val s0 = { ...emptyMiniJam,
      workers: Map(MkWorkerId(1) -> registeredWorker(MkWorkerId(1), MkAccountId(1), MinimumWorkerStake)),
      activeWorkers: Set(MkWorkerId(1))
    }
    val s1 = submitWork(
      s0,
      defaultWorkPackage(MkWorkId(1), MkAccountId(5), MkServiceId(1), serviceRoot(s0, MkServiceId(1)), serviceCodeHash(s0, MkServiceId(1))),
      0
    )
    all {
      assert(s1.works.get(MkWorkId(1)).status == InsufficientWorkers),
      assert(not(s1.assignments.keys().contains((MkWorkId(1), 0)))),
      init
    }
  }

  run equivocationTest = {
    val s0 = preparedWork
    val cand = s0.candidates.get((MkWorkId(1), 0))
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
    val s1 = castVote(castVote(s0, v1, 0), { ...v1, verdict: Oppose }, 0)
    all {
      assert(s1.equivocations.contains((MkWorkId(1), 0, MkWorkerId(1)))),
      assert(s1.workers.get(MkWorkerId(1)).suspendedUntil > s0.currentEpoch),
      init
    }
  }

  run invalidDeltaRollbackTest = {
    val pkg = defaultWorkPackage(MkWorkId(1), MkAccountId(5), MkServiceId(1), serviceRoot(initialMiniJam, MkServiceId(1)), serviceCodeHash(initialMiniJam, MkServiceId(1)))
    val report = candidateReport(pkg, 0, MkAccountId(6), List(forbiddenDelta(1)), ExecutionOk)
    val result = executeReport(initialMiniJam, report)
    all {
      assert(result._1.protocolState == initialMiniJam.protocolState),
      assert(result._1.serviceStorage == initialMiniJam.serviceStorage),
      init
    }
  }

  run bridgeInboundAtomicityTest = {
    val s0 = bridgeIn(initialMiniJam, MkAccountId(1), MkServiceId(1), 5)
    val pkg = defaultWorkPackage(MkWorkId(1), MkAccountId(5), MkServiceId(1), serviceRoot(s0, MkServiceId(1)), serviceCodeHash(s0, MkServiceId(1)))
    val report = candidateReport(pkg, 0, MkAccountId(6), List(validServiceDelta(1, 10)), ExecutionOk)
    val result = executeReport(s0, report)
    all {
      assert(result._2.orderedChanges.indices().exists(i =>
        result._2.orderedChanges[i].key.namespace == AdminBridge)),
      assert(result._1.bridge.adminRecords == Map()),
      assert(result._1.serviceStorage.get((MkServiceId(1), 1)) == 10),
      init
    }
  }

  run staleReportRejectedTest = {
    val pkg = defaultWorkPackage(MkWorkId(1), MkAccountId(5), MkServiceId(1), serviceRoot(initialMiniJam, MkServiceId(1)), serviceCodeHash(initialMiniJam, MkServiceId(1)))
    val report = { ...candidateReport(pkg, 0, MkAccountId(6), List(validServiceDelta(1, 10)), ExecutionOk),
      parentStateRoot: hashOf(999),
      postStateRoot: stateRootAfter(hashOf(999), hashOf(100))
    }
    val result = executeReport(initialMiniJam, report)
    all {
      assert(result._2.outcome == FatalInvariantError),
      assert(result._1.serviceRoots == initialMiniJam.serviceRoots),
      assert(result._1.appliedReports == Set()),
      init
    }
  }

  run reportReplayRejectedTest = {
    val pkg = defaultWorkPackage(MkWorkId(1), MkAccountId(5), MkServiceId(1), serviceRoot(initialMiniJam, MkServiceId(1)), serviceCodeHash(initialMiniJam, MkServiceId(1)))
    val report = candidateReport(pkg, 0, MkAccountId(6), List(validServiceDelta(1, 10)), ExecutionOk)
    val first = executeReport(initialMiniJam, report)
    val second = executeReport(first._1, report)
    all {
      assert(first._2.outcome == ExecutionOk),
      assert(second._2.outcome == FatalInvariantError),
      assert(second._1 == first._1),
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

  run deadlineSettlementTest = {
    val s0 = preparedWork
    val s1 = settleDeadline(s0, MkWorkId(1), 0, 2)
    all {
      assert(s1.works.get(MkWorkId(1)).status == Failed),
      assert(s1.workers.get(MkWorkerId(1)).activeStake < s0.workers.get(MkWorkerId(1)).activeStake),
      assert(s1.workers.get(MkWorkerId(2)).activeStake < s0.workers.get(MkWorkerId(2)).activeStake),
      assert(s1.workers.get(MkWorkerId(3)).activeStake < s0.workers.get(MkWorkerId(3)).activeStake),
      init
    }
  }

  run serviceLookupPreimageTest = {
    val pkg = defaultWorkPackage(MkWorkId(1), MkAccountId(5), MkServiceId(1), serviceRoot(initialMiniJam, MkServiceId(1)), serviceCodeHash(initialMiniJam, MkServiceId(1)))
    val report = candidateReport(
      pkg,
      0,
      MkAccountId(6),
      List({ key: { namespace: ServiceLookup, local: 1 }, op: Upsert, value: Some(1) }, validPreimageDelta(1, 99)),
      ExecutionOk
    )
    val result = executeReport(initialMiniJam, report)
    all {
      assert(result._2.outcome == ExecutionOk),
      assert(result._1.serviceLookups.contains((MkServiceId(1), 1))),
      assert(result._1.preimages.keys().contains(hashOf(1001))),
      init
    }
  }

  run bulletinFaultTest = {
    val content = hashOf(42)
    val s0 = storeContent(authorize(initialMiniJam, MkAccountId(1)), MkAccountId(1), content, 3)
    val s1 = injectFault(s0, content, Timeout)
    all {
      assert(fetchStatus(s1, content) == Timeout),
      assert(s1.protocolState == initialMiniJam.protocolState),
      init
    }
  }
}
