// Work queue, candidate rounds, voting settlement and execution queue.
module work {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
  import workers.* from "./workers"

  pure def submitWork(s: MiniJamState, pkg: WorkPackage, block: BlockNumber): MiniJamState = {
    if (s.works.keys().contains(pkg.workId)) s
    else if (pkg.parentStateRoot != serviceRoot(s, pkg.service) or pkg.serviceCodeHash != serviceCodeHash(s, pkg.service)) s
    else {
      val round = 0
      val rec = {
        owner: pkg.owner,
        service: pkg.service,
        package: pkg,
        round: round,
        status: if (canAssign(s, pkg.workId, round)) AwaitingCandidate else InsufficientWorkers,
        candidateDeadline: block + ReportSubmissionDeadline,
        executeAt: 0
      }
      val assn = deterministicAssignment(s, pkg.workId, round)
      {
        ...s,
        works: s.works.put(pkg.workId, rec),
        assignments: if (assn.size() == WorkersPerWork) s.assignments.put((pkg.workId, round), assn) else s.assignments
      }
    }
  }

  pure def submitCandidate(s: MiniJamState, workId: WorkId, report: ReportEnvelope, submitter: AccountId, block: BlockNumber): MiniJamState =
    if (not(s.works.keys().contains(workId))) s
    else {
      val w = s.works.get(workId)
      if (w.status != AwaitingCandidate or block > w.candidateDeadline) s
      else {
        val cand = { submitter: submitter, envelope: report, voteDeadline: block + VoteWindow }
        {
          ...s,
          works: s.works.put(workId, { ...w, status: Voting }),
          candidates: s.candidates.put((workId, w.round), cand)
        }
      }
    }

  pure def castVote(s: MiniJamState, vote: WorkerVote, block: BlockNumber): MiniJamState =
    if (not(s.assignments.keys().contains((vote.workId, vote.round))) or
        not(s.assignments.get((vote.workId, vote.round)).contains(vote.worker)) or
        not(s.candidates.keys().contains((vote.workId, vote.round))) or
        not(s.works.keys().contains(vote.workId))) s
    else {
      val candidate = s.candidates.get((vote.workId, vote.round))
      val work = s.works.get(vote.workId)
      val key = (vote.workId, vote.round, vote.worker)
      val malformed =
        vote.chainId != ChainId or
        vote.protocolVersion != ProtocolVersion or
        vote.candidateHash != candidate.envelope.reportHash or
        block > vote.deadline or
        vote.deadline != candidate.voteDeadline or
        (work.status != Voting and work.status != Accepted and work.status != Failed)
      if (malformed) s
      else if (s.votes.keys().contains(key)) {
        val old = s.votes.get(key)
        if (old.candidateHash != vote.candidateHash or old.verdict != vote.verdict)
          slashEquivocation(s, vote.workId, vote.round, vote.worker)
        else s
      } else {
        val vr0 = openVotingRound(candidate)
        val priorVotes = s.votes.keys().filter(k => k._1 == vote.workId and k._2 == vote.round)
        val vr = priorVotes.fold(vr0, (acc, k) => applyVoteToRound(acc, s.votes.get(k)))
        val nextVr = applyVoteToRound(vr, vote)
        val updatedWork =
          if (nextVr.locked == Some(RoundAccepted))
            { ...work, status: Accepted, executeAt: vote.deadline + 1 }
          else if (nextVr.locked == Some(RoundRejected))
            { ...work, status: Failed }
          else work
        val withVote = {
          ...s,
          votes: s.votes.put(key, vote),
          works: s.works.put(vote.workId, updatedWork),
          executionQueue: if (nextVr.locked == Some(RoundAccepted)) s.executionQueue.append(vote.workId) else s.executionQueue
        }
        rewardVote(withVote, vote.worker)
      }
    }

  pure def settleDeadline(s: MiniJamState, workId: WorkId, round: Round, block: BlockNumber): MiniJamState =
    if (not(s.candidates.keys().contains((workId, round))) or block < s.candidates.get((workId, round)).voteDeadline) s
    else {
      val assignedWorkers = assigned(s, workId, round)
      val responded = s.votes.keys().filter(k => k._1 == workId and k._2 == round).map(k => k._3)
      val absentees = assignedWorkers.exclude(responded)
      val afterSlash = absentees.fold(s, (acc, worker) => slashAbsence(acc, worker))
      if (not(afterSlash.works.keys().contains(workId))) afterSlash
      else {
        val rec = afterSlash.works.get(workId)
        if (rec.status == Voting)
          { ...afterSlash, works: afterSlash.works.put(workId, { ...rec, status: Failed }) }
        else afterSlash
      }
    }
}
