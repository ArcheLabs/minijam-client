// Top-level composition for the MiniJAM Quint spec.
module main {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
  import state_vars.* from "./state_vars"
  import workers.* from "./workers"
  import work.* from "./work"
  import refine.* from "./refine"
  import accumulate.* from "./accumulate"
  import bridge.* from "./bridge"
  import bulletin.* from "./bulletin"
  import invariants.* from "./invariants"

  pure val emptyExecution: ExecutionOutput = {
    orderedChanges: List(),
    consumedReports: List(),
    serviceOutputs: List(),
    bridgeEffects: List(),
    gasUsed: 0,
    receiptHash: hashOf(0),
    outcome: ExecutionOk
  }

  pure val initialWorkers: WorkerId -> WorkerRecord =
    Map(
      MkWorkerId(1) -> registeredWorker(MkWorkerId(1), MkAccountId(1), MinimumWorkerStake),
      MkWorkerId(2) -> registeredWorker(MkWorkerId(2), MkAccountId(2), MinimumWorkerStake),
      MkWorkerId(3) -> registeredWorker(MkWorkerId(3), MkAccountId(3), MinimumWorkerStake),
      MkWorkerId(4) -> registeredWorker(MkWorkerId(4), MkAccountId(4), MinimumWorkerStake)
    )

  pure val initialMiniJam: MiniJamState = {
    ...emptyMiniJam,
    workers: initialWorkers,
    activeWorkers: Set(MkWorkerId(1), MkWorkerId(2), MkWorkerId(3), MkWorkerId(4)),
    serviceRoots: Map(MkServiceId(1) -> hashOf(0)),
    serviceCodeHashes: Map(MkServiceId(1) -> hashOf(101)),
    serviceNonce: Map(MkServiceId(1) -> 0)
  }

  action init = all {
    mj' = initialMiniJam,
    now' = 0,
    prevMj' = initialMiniJam,
    lastExecution' = emptyExecution,
    lastDelta' = List(),
    lastVotes' = List(),
    lastFault' = NoBulletinFault
  }

  action stepSubmitWork = {
    nondet workId = oneOf(WorkDomain)
    val pkg = defaultWorkPackage(workId, MkAccountId(5), MkServiceId(1), serviceRoot(mj, MkServiceId(1)), serviceCodeHash(mj, MkServiceId(1)))
    val updated = submitWork(mj, pkg, now)
    all {
      mj' = updated,
      now' = now,
      prevMj' = mj,
      lastExecution' = emptyExecution,
      lastDelta' = List(),
      lastVotes' = List(),
      lastFault' = NoBulletinFault
    }
  }

  action stepSubmitCandidate = {
    nondet workId = oneOf(mj.works.keys())
    val rec = mj.works.get(workId)
    nondet deltaKind = oneOf(Set(0, 1, 2))
    val delta =
      if (deltaKind == 0) List(validServiceDelta(1, 10))
      else if (deltaKind == 1) List(validPreimageDelta(1, 99))
      else List(forbiddenDelta(1))
    val report = candidateReport(rec.package, rec.round, MkAccountId(6), delta, ExecutionOk)
    val updated = submitCandidate(mj, workId, report, MkAccountId(6), now)
    all {
      mj' = updated,
      now' = now,
      prevMj' = mj,
      lastExecution' = emptyExecution,
      lastDelta' = List(),
      lastVotes' = List(),
      lastFault' = report.bulletinFault
    }
  }

  action stepVote = {
    nondet candKey = oneOf(mj.candidates.keys())
    val workId = candKey._1
    val round = candKey._2
    nondet worker = oneOf(assigned(mj, workId, round))
    nondet verdict = oneOf(Set(Support, Oppose))
    val cand = mj.candidates.get(candKey)
    val vote: WorkerVote = {
      worker: worker,
      workId: workId,
      round: round,
      candidateHash: cand.envelope.reportHash,
      verdict: verdict,
      deadline: cand.voteDeadline,
      chainId: ChainId,
      protocolVersion: ProtocolVersion
    }
    val updated = castVote(mj, vote, now)
    all {
      mj' = updated,
      now' = now,
      prevMj' = mj,
      lastExecution' = emptyExecution,
      lastDelta' = List(),
      lastVotes' = List(vote),
      lastFault' = NoBulletinFault
    }
  }

  action stepMalformedVote = {
    nondet candKey = oneOf(mj.candidates.keys())
    val workId = candKey._1
    val round = candKey._2
    val assignedWorkers = assigned(mj, workId, round)
    val cand = mj.candidates.get(candKey)
    nondet malformedKind = oneOf(Set(0, 1, 2, 3, 4))
    nondet assignedWorker = oneOf(assignedWorkers)
    val worker =
      if (malformedKind == 3) MkWorkerId(5)
      else assignedWorker
    val vote: WorkerVote = {
      worker: worker,
      workId: workId,
      round: round,
      candidateHash: if (malformedKind == 2) hashOf(9999) else cand.envelope.reportHash,
      verdict: Support,
      deadline: if (malformedKind == 4) now - 1 else cand.voteDeadline,
      chainId: if (malformedKind == 0) hashOf(12345) else ChainId,
      protocolVersion: if (malformedKind == 1) ProtocolVersion + 1 else ProtocolVersion
    }
    val updated = castVote(mj, vote, now)
    all {
      mj' = updated,
      now' = now,
      prevMj' = mj,
      lastExecution' = emptyExecution,
      lastDelta' = List(),
      lastVotes' = List(vote),
      lastFault' = NoBulletinFault
    }
  }

  action stepExecute = {
    nondet workId = oneOf(WorkDomain)
    val inQueue = listContains(mj.executionQueue, workId)
    val dummyWork: WorkRecord = {
      owner: MkAccountId(0),
      service: MkServiceId(0),
      package: defaultWorkPackage(workId, MkAccountId(0), MkServiceId(0), hashOf(0), hashOf(0)),
      round: 0,
      status: Failed,
      candidateDeadline: 0,
      executeAt: 0
    }
    val work = if (inQueue and mj.works.keys().contains(workId)) mj.works.get(workId) else dummyWork
    val dummyReport = candidateReport(dummyWork.package, 0, MkAccountId(0), List(), Trap)
    val dummyCandidate: CandidateRecord = {
      submitter: MkAccountId(0),
      envelope: dummyReport,
      voteDeadline: 0
    }
    val cand =
      if (inQueue and mj.candidates.keys().contains((workId, work.round)))
        mj.candidates.get((workId, work.round))
      else dummyCandidate
    val result = executeReport(mj, cand.envelope)
    val executed =
      if (mj.executionPaused or result._2.outcome != ExecutionOk) result._1
      else {
        ...result._1,
        works: result._1.works.put(workId, { ...work, status: Executed }),
        executionQueue: result._1.executionQueue.select(w => w != workId)
      }
    all {
      inQueue,
      mj' = if (mj.executionPaused) mj else executed,
      now' = now,
      prevMj' = mj,
      lastExecution' = if (mj.executionPaused) emptyExecution else result._2,
      lastDelta' = if (mj.executionPaused) List() else result._2.orderedChanges,
      lastVotes' = List(),
      lastFault' = cand.envelope.bulletinFault
    }
  }

  action stepBridgeIn = {
    nondet account = oneOf(AccountDomain)
    val updated = bridgeIn(mj, account, MkServiceId(1), 5)
    all {
      mj' = updated,
      now' = now,
      prevMj' = mj,
      lastExecution' = emptyExecution,
      lastDelta' = List(),
      lastVotes' = List(),
      lastFault' = NoBulletinFault
    }
  }

  action stepBulletinFault = {
    val content = hashOf(42)
    nondet fault = oneOf(Set(Missing, Corrupt, Expired, Timeout))
    val stored = storeContent(authorize(mj, MkAccountId(1)), MkAccountId(1), content, 3)
    val updated = injectFault(stored, content, fault)
    all {
      mj' = updated,
      now' = now,
      prevMj' = mj,
      lastExecution' = emptyExecution,
      lastDelta' = List(),
      lastVotes' = List(),
      lastFault' = fetchStatus(updated, content)
    }
  }

  action stepTick = all {
    mj' = {
      ...mj,
      currentEpoch: if ((now + 1) % EpochLength == 0) mj.currentEpoch + 1 else mj.currentEpoch,
      bulletin: { ...mj.bulletin, blockNumber: mj.bulletin.blockNumber + 1 }
    },
    now' = now + 1,
    prevMj' = mj,
    lastExecution' = emptyExecution,
    lastDelta' = List(),
    lastVotes' = List(),
    lastFault' = NoBulletinFault
  }

  action step = any {
    stepSubmitWork,
    stepSubmitCandidate,
    stepVote,
    stepMalformedVote,
    stepExecute,
    stepBridgeIn,
    stepBulletinFault,
    stepTick
  }
}
