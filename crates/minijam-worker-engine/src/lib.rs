// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{collections::BTreeMap, vec::Vec};
use bounded_collections::{BoundedVec, ConstU32};
use minijam_protocol::{
    blake2_256, AssignmentRound, BlockNumber, Hash, Verdict, WorkId, WorkerId,
    MINIMUM_ABSENCE_SLASH, MINIMUM_WORKER_STAKE, OPPOSE_THRESHOLD, SUPPORT_THRESHOLD,
    TIMELY_VOTE_REWARD, WORKERS_PER_WORK,
};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

pub type AssignedWorkers = BoundedVec<WorkerId, ConstU32<3>>;
pub type VoteRecords = BoundedVec<VoteRecord, ConstU32<3>>;

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum WorkerStatus {
    Active,
    SuspendedUntil(u32),
    UnbondingUntil(u32),
    Exited,
}

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct WorkerRecord {
    pub id: WorkerId,
    pub active_stake: u128,
    pub session_key: [u8; 32],
    pub status: WorkerStatus,
}

impl WorkerRecord {
    pub fn eligible_at(&self, epoch: u32) -> bool {
        self.active_stake >= MINIMUM_WORKER_STAKE
            && match self.status {
                WorkerStatus::Active => true,
                WorkerStatus::SuspendedUntil(until) => epoch >= until,
                WorkerStatus::UnbondingUntil(_) | WorkerStatus::Exited => false,
            }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssignmentError {
    InsufficientWorkers,
    CapacityExceeded,
}

pub fn top_workers(
    workers: impl IntoIterator<Item = WorkerRecord>,
    epoch: u32,
    top_n: usize,
) -> Vec<WorkerRecord> {
    let mut eligible: Vec<_> = workers
        .into_iter()
        .filter(|worker| worker.eligible_at(epoch))
        .collect();
    eligible.sort_by(|a, b| {
        b.active_stake
            .cmp(&a.active_stake)
            .then_with(|| a.id.cmp(&b.id))
    });
    eligible.truncate(top_n);
    eligible
}

pub fn assign_batch(
    seed: Hash,
    works: &[(WorkId, AssignmentRound)],
    pool: &[WorkerRecord],
    max_duties: u32,
) -> Result<BTreeMap<WorkId, AssignedWorkers>, AssignmentError> {
    if pool.len() < WORKERS_PER_WORK as usize {
        return Err(AssignmentError::InsufficientWorkers);
    }

    let mut duties = BTreeMap::<WorkerId, u32>::new();
    let mut assignments = BTreeMap::new();
    for (work_id, round) in works {
        let mut candidates: Vec<_> = pool
            .iter()
            .filter(|worker| duties.get(&worker.id).copied().unwrap_or(0) < max_duties)
            .map(|worker| {
                let mut bytes = b"minijam/assignment-v1".to_vec();
                bytes.extend(seed);
                bytes.extend(work_id.to_le_bytes());
                bytes.push(*round);
                bytes.extend(worker.id.to_le_bytes());
                (blake2_256(&bytes), worker.id)
            })
            .collect();
        candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        if candidates.len() < WORKERS_PER_WORK as usize {
            return Err(AssignmentError::CapacityExceeded);
        }
        let selected = candidates
            .into_iter()
            .take(WORKERS_PER_WORK as usize)
            .map(|(_, worker_id)| {
                duties
                    .entry(worker_id)
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
                worker_id
            })
            .collect::<Vec<_>>();
        assignments.insert(
            *work_id,
            BoundedVec::try_from(selected).map_err(|_| AssignmentError::CapacityExceeded)?,
        );
    }
    Ok(assignments)
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct VoteRecord {
    pub worker_id: WorkerId,
    pub verdict: Verdict,
    pub submitted_at: BlockNumber,
}

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum LockedOutcome {
    Accepted,
    Rejected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VoteError {
    NotAssigned,
    DuplicateVote,
    Closed,
    BeforeCandidate,
    BoundExceeded,
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct CandidateRound {
    pub work_id: WorkId,
    pub round: AssignmentRound,
    pub assigned: AssignedWorkers,
    pub report_hash: Option<Hash>,
    pub report_deadline: BlockNumber,
    pub vote_deadline: Option<BlockNumber>,
    pub votes: VoteRecords,
    pub locked_outcome: Option<LockedOutcome>,
}

impl CandidateRound {
    pub fn new(
        work_id: WorkId,
        round: AssignmentRound,
        assigned: AssignedWorkers,
        report_deadline: BlockNumber,
    ) -> Self {
        Self {
            work_id,
            round,
            assigned,
            report_hash: None,
            report_deadline,
            vote_deadline: None,
            votes: Default::default(),
            locked_outcome: None,
        }
    }

    pub fn open_candidate(
        &mut self,
        report_hash: Hash,
        now: BlockNumber,
        vote_window: BlockNumber,
    ) -> Result<(), VoteError> {
        if self.report_hash.is_some() || now > self.report_deadline {
            return Err(VoteError::Closed);
        }
        self.report_hash = Some(report_hash);
        self.vote_deadline = Some(now.saturating_add(vote_window));
        Ok(())
    }

    pub fn record_vote(
        &mut self,
        worker_id: WorkerId,
        verdict: Verdict,
        now: BlockNumber,
    ) -> Result<Option<LockedOutcome>, VoteError> {
        let deadline = self.vote_deadline.ok_or(VoteError::BeforeCandidate)?;
        if now > deadline {
            return Err(VoteError::Closed);
        }
        if !self.assigned.contains(&worker_id) {
            return Err(VoteError::NotAssigned);
        }
        if self.votes.iter().any(|vote| vote.worker_id == worker_id) {
            return Err(VoteError::DuplicateVote);
        }
        self.votes
            .try_push(VoteRecord {
                worker_id,
                verdict,
                submitted_at: now,
            })
            .map_err(|_| VoteError::BoundExceeded)?;
        self.update_locked_outcome();
        Ok(self.locked_outcome)
    }

    pub fn ready_to_finalize(&self, now: BlockNumber) -> bool {
        self.vote_deadline
            .is_some_and(|deadline| now >= deadline || self.votes.len() == self.assigned.len())
    }

    pub fn final_outcome(&self, now: BlockNumber) -> Option<LockedOutcome> {
        self.ready_to_finalize(now)
            .then_some(self.locked_outcome.unwrap_or(LockedOutcome::Rejected))
    }

    pub fn absent_workers(&self, now: BlockNumber) -> Vec<WorkerId> {
        if !self.ready_to_finalize(now) {
            return Vec::new();
        }
        self.assigned
            .iter()
            .copied()
            .filter(|worker| !self.votes.iter().any(|vote| vote.worker_id == *worker))
            .collect()
    }

    fn update_locked_outcome(&mut self) {
        let support = self
            .votes
            .iter()
            .filter(|vote| matches!(vote.verdict, Verdict::Support))
            .count() as u32;
        let oppose = self
            .votes
            .iter()
            .filter(|vote| matches!(vote.verdict, Verdict::Oppose(_)))
            .count() as u32;
        if support >= SUPPORT_THRESHOLD {
            self.locked_outcome = Some(LockedOutcome::Accepted);
        } else if oppose >= OPPOSE_THRESHOLD {
            self.locked_outcome = Some(LockedOutcome::Rejected);
        }
    }
}

pub fn timely_vote_reward() -> u128 {
    TIMELY_VOTE_REWARD
}

pub fn absence_slash(stake: u128) -> u128 {
    stake
        .saturating_div(100)
        .max(MINIMUM_ABSENCE_SLASH)
        .min(stake)
}

pub fn equivocation_slash(stake: u128) -> u128 {
    stake.saturating_mul(20).saturating_div(100)
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijam_protocol::{OpposeReason, UNIT, VOTE_WINDOW};

    fn worker(id: WorkerId, stake: u128) -> WorkerRecord {
        WorkerRecord {
            id,
            active_stake: stake,
            session_key: [id as u8; 32],
            status: WorkerStatus::Active,
        }
    }

    #[test]
    fn ranking_is_stake_then_id() {
        let selected = top_workers(
            [
                worker(3, 2_000 * UNIT),
                worker(2, 2_000 * UNIT),
                worker(1, UNIT),
            ],
            0,
            2,
        );
        assert_eq!(selected.iter().map(|w| w.id).collect::<Vec<_>>(), [2, 3]);
    }

    #[test]
    fn assignment_is_deterministic_and_distinct() {
        let pool = (0..8)
            .map(|id| worker(id, (10_000 - id as u128) * UNIT))
            .collect::<Vec<_>>();
        let works = [(1, 0), (2, 0), (3, 0), (4, 0)];
        let first = assign_batch([7u8; 32], &works, &pool, 2).unwrap();
        let second = assign_batch([7u8; 32], &works, &pool, 2).unwrap();
        assert_eq!(first, second);
        assert!(first.values().all(|assigned| {
            assigned.len() == 3
                && assigned[0] != assigned[1]
                && assigned[1] != assigned[2]
                && assigned[0] != assigned[2]
        }));
    }

    #[test]
    fn support_locks_but_attendance_waits() {
        let assigned = BoundedVec::try_from(vec![1, 2, 3]).unwrap();
        let mut round = CandidateRound::new(9, 0, assigned, 20);
        round.open_candidate([4u8; 32], 5, VOTE_WINDOW).unwrap();
        assert_eq!(round.record_vote(1, Verdict::Support, 6).unwrap(), None);
        assert_eq!(
            round.record_vote(2, Verdict::Support, 7).unwrap(),
            Some(LockedOutcome::Accepted)
        );
        assert!(!round.ready_to_finalize(7));
        assert_eq!(round.absent_workers(15), vec![3]);
    }

    #[test]
    fn opposition_counts_as_attendance() {
        let assigned = BoundedVec::try_from(vec![1, 2, 3]).unwrap();
        let mut round = CandidateRound::new(9, 0, assigned, 20);
        round.open_candidate([4u8; 32], 5, VOTE_WINDOW).unwrap();
        round
            .record_vote(1, Verdict::Oppose(OpposeReason::InvalidRefine), 6)
            .unwrap();
        round
            .record_vote(2, Verdict::Oppose(OpposeReason::MissingData), 7)
            .unwrap();
        round.record_vote(3, Verdict::Support, 8).unwrap();
        assert_eq!(round.final_outcome(8), Some(LockedOutcome::Rejected));
        assert!(round.absent_workers(8).is_empty());
    }
}
