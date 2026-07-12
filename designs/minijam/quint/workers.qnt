// Worker registration, epoch snapshots, assignment and voting.
module workers {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
  import state_vars.* from "./state_vars"

  pure def registeredWorker(id: WorkerId, owner: AccountId, stake: Balance): WorkerRecord = {
    owner: owner,
    activeStake: stake,
    effectiveEpoch: 0,
    suspendedUntil: 0,
    active: true
  }

  pure def snapshotTopWorkers(s: MiniJamState): Set[WorkerId] =
    WorkerDomain.filter(w =>
      s.workers.keys().contains(w) and
      s.workers.get(w).active and
      s.workers.get(w).activeStake >= MinimumWorkerStake and
      s.workers.get(w).suspendedUntil <= s.currentEpoch)
      .filter(w => match w { | MkWorkerId(i) => i <= TopWorkers | _WorkerIdSentinel => false })

  pure def deterministicAssignment(s: MiniJamState, workId: WorkId, round: Round): Set[WorkerId] =
    snapshotTopWorkers(s).filter(w =>
      match w {
        | MkWorkerId(i) => i <= WorkersPerWork
        | _WorkerIdSentinel => false
      })

  pure def canAssign(s: MiniJamState, workId: WorkId, round: Round): bool =
    deterministicAssignment(s, workId, round).size() == WorkersPerWork

  pure def openVotingRound(candidate: CandidateRecord): VotingRound = {
    candidateHash: candidate.envelope.reportHash,
    deadline: candidate.voteDeadline,
    support: 0,
    oppose: 0,
    responses: Set(),
    locked: None
  }

  pure def lockDecision(vr: VotingRound): Option[RoundDecision] =
    if (vr.support >= SupportThreshold) Some(RoundAccepted)
    else if (vr.oppose >= OpposeThreshold) Some(RoundRejected)
    else None

  pure def applyVoteToRound(vr: VotingRound, vote: WorkerVote): VotingRound = {
    val nextSupport = if (vote.verdict == Support) vr.support + 1 else vr.support
    val nextOppose = if (vote.verdict == Oppose) vr.oppose + 1 else vr.oppose
    val updated = {
      ...vr,
      support: nextSupport,
      oppose: nextOppose,
      responses: vr.responses.union(Set(vote.worker))
    }
    { ...updated, locked: lockDecision(updated) }
  }

  pure def rewardVote(s: MiniJamState, worker: WorkerId): MiniJamState =
    if (s.rewardPool >= TimelyVoteReward)
      { ...s, rewardPool: s.rewardPool - TimelyVoteReward }
    else s

  pure def slashAbsence(s: MiniJamState, worker: WorkerId): MiniJamState =
    if (not(s.workers.keys().contains(worker))) s
    else {
      val rec = s.workers.get(worker)
      val slash = if (rec.activeStake >= AbsenceSlash) AbsenceSlash else rec.activeStake
      { ...s, workers: s.workers.put(worker, { ...rec, activeStake: rec.activeStake - slash }) }
    }

  pure def slashEquivocation(s: MiniJamState, workId: WorkId, round: Round, worker: WorkerId): MiniJamState =
    if (not(s.workers.keys().contains(worker))) s
    else {
      val rec = s.workers.get(worker)
      val slash = if (rec.activeStake >= EquivocationSlash) EquivocationSlash else rec.activeStake
      {
        ...s,
        workers: s.workers.put(worker, {
          ...rec,
          activeStake: rec.activeStake - slash,
          suspendedUntil: s.currentEpoch + EquivocationSuspension
        }),
        equivocations: s.equivocations.union(Set((workId, round, worker)))
      }
    }
}
