// Nullary invariant catalogue for the MiniJAM model.
module invariants {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
  import executive.* from "./executive"
  import state_vars.* from "./state_vars"

  pure def voteWellFormedIn(s: MiniJamState, vote: WorkerVote, block: BlockNumber): bool =
    s.assignments.keys().contains((vote.workId, vote.round)) and
    s.assignments.get((vote.workId, vote.round)).contains(vote.worker) and
    s.candidates.keys().contains((vote.workId, vote.round)) and
    s.works.keys().contains(vote.workId) and
    vote.chainId == ChainId and
    vote.protocolVersion == ProtocolVersion and
    vote.candidateHash == s.candidates.get((vote.workId, vote.round)).envelope.reportHash and
    vote.deadline == s.candidates.get((vote.workId, vote.round)).voteDeadline and
    block <= vote.deadline

  val assignment_invariant: bool =
    mj.assignments.keys().forall(k =>
      mj.assignments.get(k).size() == WorkersPerWork and
      mj.assignments.get(k).forall(w =>
        mj.workers.keys().contains(w) and mj.activeWorkers.contains(w)))

  val insufficient_workers_invariant: bool =
    mj.works.keys().forall(workId => {
      val w = mj.works.get(workId)
      w.status != InsufficientWorkers or
      not(mj.assignments.keys().contains((workId, w.round)))
    })

  val vote_threshold_invariant: bool =
    mj.works.keys().forall(workId => {
      val work = mj.works.get(workId)
      val votes = mj.votes.keys().filter(k => k._1 == workId and k._2 == work.round)
      val supports = votes.filter(k => mj.votes.get(k).verdict == Support).size()
      val opposes = votes.filter(k => mj.votes.get(k).verdict == Oppose).size()
      (work.status != Accepted and work.status != Imported or supports >= SupportThreshold) and
      (work.status != Failed or opposes >= OpposeThreshold or work.round >= MaxCandidateRounds)
    })

  val single_vote_or_equivocation_invariant: bool =
    mj.votes.keys().forall(k =>
      not(mj.equivocations.contains(k)) or mj.workers.keys().contains(k._3))

  val execution_requires_acceptance_invariant: bool =
    mj.executionQueue.indices().forall(i =>
      mj.works.keys().contains(mj.executionQueue[i]) and
      mj.works.get(mj.executionQueue[i]).status == Accepted)

  val paused_or_quarantined_not_imported_invariant: bool =
    if (mj.executionPaused) mj == prevMj or lastExecution.importedReports == Set()
    else mj.quarantinedExecutionQueue.indices().forall(i =>
      mj.works.keys().contains(mj.quarantinedExecutionQueue[i]) and
      mj.works.get(mj.quarantinedExecutionQueue[i]).status != Imported)

  val virtual_block_atomicity_invariant: bool =
    if (lastExecution.outcome != ExecutionOk and lastExecution.phases == List(Start))
      mj.jam == prevMj.jam and
      mj.executionQueue == prevMj.executionQueue and
      mj.pendingPreimages == prevMj.pendingPreimages
    else true

  val retained_stf_order_invariant: bool =
    lastExecution.outcome != ExecutionOk or lastExecution.phases == RetainedPhaseOrder

  val tickets_always_empty_invariant: bool =
    lastExecution.ticketsUsed == Set()

  val removed_stfs_never_run_invariant: bool =
    lastExecution.removedStfs == RemovedStfs

  val reports_bypass_assurance_invariant: bool =
    lastExecution.outcome != ExecutionOk or
    mj.jam.accumulateInputReports == lastExecution.importedReports

  val reports_bypass_guarantee_invariant: bool =
    lastExecution.outcome != ExecutionOk or
    lastExecution.removedStfs.contains(ReportsGuarantees)

  val history_uses_imported_reports_invariant: bool =
    lastExecution.outcome != ExecutionOk or
    mj.jam.currentHistoryPackages ==
      lastExecution.importedReports.map(h => {
        val report = mj.acceptedReports.get(h)
        report.metadata.packageHash
      })

  val authorization_uses_imported_reports_invariant: bool =
    lastExecution.outcome != ExecutionOk or
    mj.jam.authorizationConsumed ==
      lastExecution.importedReports.map(h => mj.acceptedReports.get(h).canonical.authorizerHash)

  val preimage_snapshot_invariant: bool =
    lastExecution.outcome != ExecutionOk or
    lastExecution.importedPreimages.subseteq(prevMj.pendingPreimages.indices().map(i => prevMj.pendingPreimages[i].preimageHash))

  val runtime_import_not_accumulation_invariant: bool =
    lastExecution.outcome != ExecutionOk or
    lastExecution.importedReports.forall(reportHash =>
      mj.works.keys().exists(workId =>
        mj.works.get(workId).status == Imported and
        mj.acceptedReports.keys().contains(reportHash))
    )

  val malformed_vote_rejected_invariant: bool =
    lastVotes.length() == 0 or
    lastVotes.indices().forall(i =>
      voteWellFormedIn(prevMj, lastVotes[i], now) or mj == prevMj)

  val bridge_nonce_invariant: bool =
    mj.bridge.nextInboundNonce >= prevMj.bridge.nextInboundNonce and
    mj.bridge.consumedOutbound.size() >= prevMj.bridge.consumedOutbound.size()

  val bridge_escrow_invariant: bool =
    mj.bridge.escrow.keys().forall(account => mj.bridge.escrow.get(account) >= 0)

  val reward_slash_conservation_invariant: bool =
    mj.rewardPool >= 0 and
    mj.workers.keys().forall(w => mj.workers.get(w).activeStake >= 0)

  val bulletin_fault_isolation_invariant: bool =
    lastFault == NoBulletinFault or mj.jam == prevMj.jam

  val invariants: bool =
    assignment_invariant and
    insufficient_workers_invariant and
    vote_threshold_invariant and
    single_vote_or_equivocation_invariant and
    execution_requires_acceptance_invariant and
    paused_or_quarantined_not_imported_invariant and
    virtual_block_atomicity_invariant and
    retained_stf_order_invariant and
    tickets_always_empty_invariant and
    removed_stfs_never_run_invariant and
    reports_bypass_assurance_invariant and
    reports_bypass_guarantee_invariant and
    history_uses_imported_reports_invariant and
    authorization_uses_imported_reports_invariant and
    preimage_snapshot_invariant and
    runtime_import_not_accumulation_invariant and
    malformed_vote_rejected_invariant and
    bridge_nonce_invariant and
    bridge_escrow_invariant and
    reward_slash_conservation_invariant and
    bulletin_fault_isolation_invariant
}
