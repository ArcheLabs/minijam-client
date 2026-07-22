// Top-level composition for the MiniJAM Quint spec.
module main {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
  import state_vars.* from "./state_vars"
  import workers.* from "./workers"
  import work.* from "./work"
  import refine.* from "./refine"
  import executive.* from "./executive"
  import bridge.* from "./bridge"
  import bulletin.* from "./bulletin"
  import invariants.* from "./invariants"

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
    activeWorkers: Set(MkWorkerId(1), MkWorkerId(2), MkWorkerId(3), MkWorkerId(4))
  }

  pure def consumePendingPreimages(s: MiniJamState, hashes: Set[Hash]): MiniJamState =
    { ...s, pendingPreimages: s.pendingPreimages.select(p => not(hashes.contains(p.preimageHash))) }

  pure def markImportedWork(s: MiniJamState, workId: WorkId, output: ExecutionOutput): MiniJamState =
    if (not(s.works.keys().contains(workId))) s
    else {
      val work = s.works.get(workId)
      val cand = s.candidates.get((workId, work.round))
      if (output.importedReports.contains(cand.envelope.reportHash))
        {
          ...s,
          works: s.works.put(workId, { ...work, status: Imported }),
          executionQueue: s.executionQueue.select(w => w != workId)
        }
      else s
    }

  pure def executionInputFor(s: MiniJamState, workId: WorkId, slot: BlockNumber): ExecutionInput = {
    val work = s.works.get(workId)
    val cand = s.candidates.get((workId, work.round))
    {
      protocolVersion: ProtocolVersion,
      slot: slot,
      parentHash: hashOf(slot + 10),
      parentStateRoot: hashOf(slot + 20),
      entropy: hashOf(slot + 30),
      reports: List(cand.envelope.canonical),
      preimages: s.pendingPreimages,
      maxGas: MaxGas
    }
  }

  action init = all {
    mj' = initialMiniJam,
    now' = 0,
    prevMj' = initialMiniJam,
    lastExecution' = emptyExecution,
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
      lastVotes' = List(),
      lastFault' = NoBulletinFault
    }
  }

  action stepSubmitCandidate = {
    nondet workId = oneOf(mj.works.keys())
    val rec = mj.works.get(workId)
    nondet dependencyKind = oneOf(Set(0, 1))
    val deps = if (dependencyKind == 0) Set() else Set(hashOf(999))
    val report = candidateReport(rec.package, rec.round, MkAccountId(6), deps, 10)
    val updated = submitCandidate(mj, workId, report, MkAccountId(6), now)
    all {
      mj' = updated,
      now' = now,
      prevMj' = mj,
      lastExecution' = emptyExecution,
      lastVotes' = List(),
      lastFault' = report.bulletinFault
    }
  }

  action stepSubmitPreimage = {
    nondet id = oneOf(Set(1, 2, 3))
    val preimage: CanonicalPreimage = {
      preimageHash: hashOf(500 + id),
      requester: MkServiceId(1),
      blobLength: id
    }
    all {
      mj' = queuePreimage(mj, preimage),
      now' = now,
      prevMj' = mj,
      lastExecution' = emptyExecution,
      lastVotes' = List(),
      lastFault' = NoBulletinFault
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
      lastVotes' = List(vote),
      lastFault' = NoBulletinFault
    }
  }

  action stepExecute = {
    nondet workId = oneOf(WorkDomain)
    val inQueue = listContains(mj.executionQueue, workId)
    val input = if (inQueue) executionInputFor(mj, workId, now + 1) else {
      protocolVersion: ProtocolVersion,
      slot: now + 1,
      parentHash: hashOf(0),
      parentStateRoot: hashOf(0),
      entropy: hashOf(0),
      reports: List(),
      preimages: mj.pendingPreimages,
      maxGas: MaxGas
    }
    val result = executeVirtualJamBlock(mj, input)
    val imported =
      if (mj.executionPaused or result._2.outcome != ExecutionOk) result._1
      else consumePendingPreimages(markImportedWork(result._1, workId, result._2), result._2.importedPreimages)
    all {
      inQueue,
      mj' = if (mj.executionPaused) mj else imported,
      now' = now,
      prevMj' = mj,
      lastExecution' = if (mj.executionPaused) emptyExecution else result._2,
      lastVotes' = List(),
      lastFault' = NoBulletinFault
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
    lastVotes' = List(),
    lastFault' = NoBulletinFault
  }

  action step = any {
    stepSubmitWork,
    stepSubmitCandidate,
    stepSubmitPreimage,
    stepVote,
    stepMalformedVote,
    stepExecute,
    stepBridgeIn,
    stepBulletinFault,
    stepTick
  }
}
