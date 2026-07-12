// Nullary invariant catalogue for the MiniJAM model.
module invariants {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
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
      (work.status != Accepted or supports >= SupportThreshold) and
      (work.status != Failed or opposes >= OpposeThreshold or work.round >= MaxCandidateRounds)
    })

  val single_vote_or_equivocation_invariant: bool =
    mj.votes.keys().forall(k =>
      not(mj.equivocations.contains(k)) or mj.workers.keys().contains(k._3))

  val execution_requires_acceptance_invariant: bool =
    mj.executionQueue.indices().forall(i =>
      mj.works.keys().contains(mj.executionQueue[i]) and
      mj.works.get(mj.executionQueue[i]).status == Accepted)

  val paused_or_quarantined_not_executed_invariant: bool =
    if (mj.executionPaused) mj == prevMj or lastExecution.consumedReports == List()
    else mj.quarantinedExecutionQueue.indices().forall(i =>
      mj.works.keys().contains(mj.quarantinedExecutionQueue[i]) and
      mj.works.get(mj.quarantinedExecutionQueue[i]).status != Executed)

  val delta_allowlist_invariant: bool =
    lastExecution.outcome != ExecutionOk or
    lastDelta.indices().forall(i => namespaceAllowed(lastDelta[i].key.namespace))

  val delta_order_unique_invariant: bool =
    lastExecution.outcome != ExecutionOk or changesOrderedUnique(lastDelta)

  val execution_atomicity_invariant: bool =
    if (lastExecution.outcome == FatalStateError or lastExecution.outcome == FatalInvariantError)
      mj.protocolState == prevMj.protocolState and
      mj.serviceStorage == prevMj.serviceStorage and
      mj.serviceRoots == prevMj.serviceRoots and
      mj.bridge.adminRecords == prevMj.bridge.adminRecords and
      mj.appliedReports == prevMj.appliedReports
    else true

  val service_root_continuity_invariant: bool =
    mj.appliedReports.forall(reportHash => {
      val report = mj.acceptedReports.get(reportHash)
      report.parentStateRoot != report.postStateRoot and
      report.postStateRoot == stateRootAfter(report.parentStateRoot, report.reportHash)
    })

  val accepted_report_parent_continuity_invariant: bool =
    lastExecution.outcome != ExecutionOk or
    lastExecution.consumedReports.indices().forall(i => {
      val reportHash = lastExecution.consumedReports[i]
      mj.acceptedReports.keys().contains(reportHash) and
      mj.serviceRoots.get(mj.acceptedReports.get(reportHash).service) ==
        mj.acceptedReports.get(reportHash).postStateRoot
    })

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

  val preimage_consistency_invariant: bool =
    mj.serviceLookups.forall(k =>
      mj.preimages.keys().contains(hashOf(1000 + k._2)) or
      lastFault == Missing or lastFault == Expired or lastFault == Timeout or lastFault == Corrupt)

  val export_determinism_invariant: bool =
    mj.appliedReports.forall(reportHash => {
      val report = mj.acceptedReports.get(reportHash)
      mj.exports.keys().contains((report.workId, report.metadata.exportsRoot)) and
      mj.exports.get((report.workId, report.metadata.exportsRoot)) == report.metadata.exportsRoot
    })

  val invariants: bool =
    assignment_invariant and
    insufficient_workers_invariant and
    vote_threshold_invariant and
    single_vote_or_equivocation_invariant and
    execution_requires_acceptance_invariant and
    paused_or_quarantined_not_executed_invariant and
    delta_allowlist_invariant and
    delta_order_unique_invariant and
    execution_atomicity_invariant and
    service_root_continuity_invariant and
    accepted_report_parent_continuity_invariant and
    malformed_vote_rejected_invariant and
    bridge_nonce_invariant and
    bridge_escrow_invariant and
    reward_slash_conservation_invariant and
    preimage_consistency_invariant and
    export_determinism_invariant
}
