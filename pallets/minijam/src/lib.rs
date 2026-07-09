// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use alloc::boxed::Box;
    use frame_support::{
        pallet_prelude::*,
        storage::{with_transaction, TransactionOutcome},
        traits::tokens::{
            fungible::{Balanced, BalancedHold, Inspect, Mutate, MutateHold},
            Precision, Preservation,
        },
        transactional,
    };
    use frame_system::pallet_prelude::*;
    use minijam_protocol::{ReportEnvelopeV1, PROTOCOL_VERSION_V1};
    use pallet_minijam_workers::RoundDecision;
    use sp_runtime::traits::{SaturatedConversion, Saturating, Zero};

    pub type WorkId = u64;
    pub type BalanceOf<T> =
        <<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

    #[derive(
        Clone,
        Copy,
        Debug,
        Decode,
        DecodeWithMemTracking,
        Encode,
        Eq,
        MaxEncodedLen,
        PartialEq,
        TypeInfo,
    )]
    pub enum WorkStatus {
        InsufficientWorkers,
        AwaitingCandidate,
        Voting,
        Accepted,
        Failed,
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    #[scale_info(skip_type_params(T))]
    pub struct WorkRecord<T: Config> {
        pub owner: T::AccountId,
        pub round: u8,
        pub status: WorkStatus,
        pub candidate_deadline: BlockNumberFor<T>,
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    #[scale_info(skip_type_params(T))]
    pub struct CandidateRecord<T: Config> {
        pub submitter: T::AccountId,
        pub envelope: ReportEnvelopeV1,
        pub vote_deadline: BlockNumberFor<T>,
    }

    #[pallet::composite_enum]
    pub enum HoldReason {
        WorkDeposit,
        CandidateBond,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_minijam_workers::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Currency: Mutate<Self::AccountId>
            + MutateHold<Self::AccountId, Reason = Self::JamHoldReason>
            + BalancedHold<Self::AccountId, Reason = Self::JamHoldReason>;

        type JamHoldReason: From<HoldReason>;

        #[pallet::constant]
        type ChainId: Get<[u8; 32]>;

        #[pallet::constant]
        type WorkDeposit: Get<BalanceOf<Self>>;

        #[pallet::constant]
        type CandidateBond: Get<BalanceOf<Self>>;

        #[pallet::constant]
        type CandidateRejectionSlash: Get<BalanceOf<Self>>;

        #[pallet::constant]
        type AcceptedSubmitterReward: Get<BalanceOf<Self>>;

        #[pallet::constant]
        type RewardPool: Get<Self::AccountId>;

        #[pallet::constant]
        type ReportSubmissionDeadline: Get<u32>;

        #[pallet::constant]
        type VoteWindow: Get<u32>;

        #[pallet::constant]
        type MaxCandidateRounds: Get<u8>;

        #[pallet::constant]
        type MaxPendingWorks: Get<u32>;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type NextWorkId<T> = StorageValue<_, WorkId, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn work)]
    pub type Works<T: Config> = StorageMap<_, Blake2_128Concat, WorkId, WorkRecord<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn candidate)]
    pub type Candidates<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        WorkId,
        Blake2_128Concat,
        u8,
        CandidateRecord<T>,
        OptionQuery,
    >;

    #[pallet::storage]
    pub type PendingWorks<T: Config> =
        StorageValue<_, BoundedVec<WorkId, T::MaxPendingWorks>, ValueQuery>;

    #[pallet::storage]
    pub type ExecutionQueue<T: Config> =
        StorageValue<_, BoundedVec<WorkId, T::MaxPendingWorks>, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        WorkSubmitted {
            work_id: WorkId,
            owner: T::AccountId,
            status: WorkStatus,
        },
        CandidateSubmitted {
            work_id: WorkId,
            round: u8,
            submitter: T::AccountId,
            report_hash: [u8; 32],
        },
        CandidateAccepted {
            work_id: WorkId,
            round: u8,
        },
        CandidateRejected {
            work_id: WorkId,
            round: u8,
        },
        WorkRoundAdvanced {
            work_id: WorkId,
            round: u8,
            status: WorkStatus,
        },
        WorkFailed {
            work_id: WorkId,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        WorkIdOverflow,
        TooManyPendingWorks,
        WorkNotFound,
        CandidateNotExpected,
        CandidateAlreadySubmitted,
        CandidateDeadlineExpired,
        InvalidEnvelope,
        InvalidReportHash,
        VotingSetupFailed,
        InconsistentState,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(block: BlockNumberFor<T>) -> Weight {
            let pending = PendingWorks::<T>::get();
            for work_id in pending {
                let Some(work) = Works::<T>::get(work_id) else {
                    continue;
                };
                match work.status {
                    WorkStatus::InsufficientWorkers => {
                        let _ = Self::prepare_round(work_id);
                    }
                    WorkStatus::AwaitingCandidate if block > work.candidate_deadline => {
                        let _ = Self::advance_or_fail(work_id, false);
                    }
                    WorkStatus::Voting => {
                        if let Some(result) =
                            pallet_minijam_workers::RoundResults::<T>::get((work_id, work.round))
                        {
                            let _ = match result.decision {
                                Some(RoundDecision::Accepted) => Self::accept_candidate(work_id),
                                Some(RoundDecision::Rejected) | None => {
                                    Self::advance_or_fail(work_id, true)
                                }
                            };
                        }
                    }
                    _ => {}
                }
            }
            T::DbWeight::get().reads_writes(
                u64::from(T::MaxPendingWorks::get()).saturating_mul(4),
                u64::from(T::MaxPendingWorks::get()).saturating_mul(4),
            )
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::DbWeight::get().reads_writes(5, 6))]
        #[transactional]
        pub fn submit_work(origin: OriginFor<T>) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            let work_id = NextWorkId::<T>::get();
            let next = work_id.checked_add(1).ok_or(Error::<T>::WorkIdOverflow)?;
            PendingWorks::<T>::try_mutate(|pending| {
                pending
                    .try_push(work_id)
                    .map_err(|_| Error::<T>::TooManyPendingWorks)
            })?;
            let reason = T::JamHoldReason::from(HoldReason::WorkDeposit);
            <T as Config>::Currency::hold(&reason, &owner, T::WorkDeposit::get())?;

            Works::<T>::insert(
                work_id,
                WorkRecord::<T> {
                    owner: owner.clone(),
                    round: 0,
                    status: WorkStatus::InsufficientWorkers,
                    candidate_deadline: Zero::zero(),
                },
            );
            NextWorkId::<T>::put(next);
            let _ = Self::prepare_round(work_id);
            let status = Works::<T>::get(work_id)
                .ok_or(Error::<T>::InconsistentState)?
                .status;
            Self::deposit_event(Event::WorkSubmitted {
                work_id,
                owner,
                status,
            });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::DbWeight::get().reads_writes(8, 6))]
        #[transactional]
        pub fn submit_candidate(
            origin: OriginFor<T>,
            envelope: Box<ReportEnvelopeV1>,
        ) -> DispatchResult {
            let submitter = ensure_signed(origin)?;
            let envelope = *envelope;
            let mut work = Works::<T>::get(envelope.work_id).ok_or(Error::<T>::WorkNotFound)?;
            ensure!(
                work.status == WorkStatus::AwaitingCandidate,
                Error::<T>::CandidateNotExpected
            );
            ensure!(
                frame_system::Pallet::<T>::block_number() <= work.candidate_deadline,
                Error::<T>::CandidateDeadlineExpired
            );
            ensure!(
                !Candidates::<T>::contains_key(envelope.work_id, work.round),
                Error::<T>::CandidateAlreadySubmitted
            );
            ensure!(
                envelope.protocol_version == PROTOCOL_VERSION_V1
                    && envelope.chain_id == <T as Config>::ChainId::get()
                    && envelope.assignment_round == work.round,
                Error::<T>::InvalidEnvelope
            );
            ensure!(
                envelope.computed_report_hash() == envelope.canonical_report_hash,
                Error::<T>::InvalidReportHash
            );

            let reason = T::JamHoldReason::from(HoldReason::CandidateBond);
            <T as Config>::Currency::hold(&reason, &submitter, T::CandidateBond::get())?;
            let vote_deadline = frame_system::Pallet::<T>::block_number()
                .saturating_add(T::VoteWindow::get().saturated_into());
            pallet_minijam_workers::Pallet::<T>::open_voting(
                envelope.work_id,
                work.round,
                envelope.canonical_report_hash,
                vote_deadline,
            )
            .map_err(|_| Error::<T>::VotingSetupFailed)?;

            let work_id = envelope.work_id;
            let report_hash = envelope.canonical_report_hash;
            Candidates::<T>::insert(
                work_id,
                work.round,
                CandidateRecord::<T> {
                    submitter: submitter.clone(),
                    envelope,
                    vote_deadline,
                },
            );
            work.status = WorkStatus::Voting;
            Works::<T>::insert(work_id, &work);
            Self::deposit_event(Event::CandidateSubmitted {
                work_id,
                round: work.round,
                submitter,
                report_hash,
            });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn prepare_round(work_id: WorkId) -> DispatchResult {
            let mut work = Works::<T>::get(work_id).ok_or(Error::<T>::WorkNotFound)?;
            match pallet_minijam_workers::Pallet::<T>::assign_work(work_id, work.round) {
                Ok(_) => {
                    work.status = WorkStatus::AwaitingCandidate;
                    work.candidate_deadline = frame_system::Pallet::<T>::block_number()
                        .saturating_add(T::ReportSubmissionDeadline::get().saturated_into());
                }
                Err(_) => {
                    work.status = WorkStatus::InsufficientWorkers;
                }
            }
            let status = work.status;
            let round = work.round;
            Works::<T>::insert(work_id, work);
            Self::deposit_event(Event::WorkRoundAdvanced {
                work_id,
                round,
                status,
            });
            Ok(())
        }

        fn advance_or_fail(work_id: WorkId, rejected_candidate: bool) -> DispatchResult {
            with_transaction(|| {
                let result = Self::advance_or_fail_inner(work_id, rejected_candidate);
                match result {
                    Ok(()) => TransactionOutcome::Commit(Ok(())),
                    Err(error) => TransactionOutcome::Rollback(Err(error)),
                }
            })
        }

        fn advance_or_fail_inner(work_id: WorkId, rejected_candidate: bool) -> DispatchResult {
            let mut work = Works::<T>::get(work_id).ok_or(Error::<T>::WorkNotFound)?;
            if rejected_candidate {
                Self::settle_rejected_candidate(work_id, work.round)?;
                Self::deposit_event(Event::CandidateRejected {
                    work_id,
                    round: work.round,
                });
            }
            if work.round.saturating_add(1) >= T::MaxCandidateRounds::get() {
                Self::fail_work(work_id, &mut work)?;
                return Ok(());
            }
            work.round = work.round.saturating_add(1);
            work.status = WorkStatus::InsufficientWorkers;
            Works::<T>::insert(work_id, &work);
            Self::prepare_round(work_id)
        }

        fn settle_rejected_candidate(work_id: WorkId, round: u8) -> DispatchResult {
            let candidate =
                Candidates::<T>::get(work_id, round).ok_or(Error::<T>::InconsistentState)?;
            let reason = T::JamHoldReason::from(HoldReason::CandidateBond);
            let slash = T::CandidateRejectionSlash::get().min(T::CandidateBond::get());
            let (credit, remainder) =
                <T as Config>::Currency::slash(&reason, &candidate.submitter, slash);
            ensure!(remainder.is_zero(), Error::<T>::InconsistentState);
            if <T as Config>::Currency::resolve(&<T as Config>::RewardPool::get(), credit).is_err()
            {
                return Err(Error::<T>::InconsistentState.into());
            }
            <T as Config>::Currency::release(
                &reason,
                &candidate.submitter,
                T::CandidateBond::get() - slash,
                Precision::Exact,
            )?;
            Ok(())
        }

        fn accept_candidate(work_id: WorkId) -> DispatchResult {
            with_transaction(|| {
                let result = Self::accept_candidate_inner(work_id);
                match result {
                    Ok(()) => TransactionOutcome::Commit(Ok(())),
                    Err(error) => TransactionOutcome::Rollback(Err(error)),
                }
            })
        }

        fn accept_candidate_inner(work_id: WorkId) -> DispatchResult {
            let mut work = Works::<T>::get(work_id).ok_or(Error::<T>::WorkNotFound)?;
            let candidate =
                Candidates::<T>::get(work_id, work.round).ok_or(Error::<T>::InconsistentState)?;
            let candidate_reason = T::JamHoldReason::from(HoldReason::CandidateBond);
            <T as Config>::Currency::release(
                &candidate_reason,
                &candidate.submitter,
                T::CandidateBond::get(),
                Precision::Exact,
            )?;
            <T as Config>::Currency::transfer(
                &<T as Config>::RewardPool::get(),
                &candidate.submitter,
                T::AcceptedSubmitterReward::get(),
                Preservation::Preserve,
            )?;
            let work_reason = T::JamHoldReason::from(HoldReason::WorkDeposit);
            <T as Config>::Currency::release(
                &work_reason,
                &work.owner,
                T::WorkDeposit::get(),
                Precision::Exact,
            )?;
            work.status = WorkStatus::Accepted;
            Works::<T>::insert(work_id, &work);
            ExecutionQueue::<T>::try_mutate(|queue| {
                queue
                    .try_push(work_id)
                    .map_err(|_| Error::<T>::TooManyPendingWorks)
            })?;
            Self::remove_pending(work_id);
            Self::deposit_event(Event::CandidateAccepted {
                work_id,
                round: work.round,
            });
            Ok(())
        }

        fn fail_work(work_id: WorkId, work: &mut WorkRecord<T>) -> DispatchResult {
            let reason = T::JamHoldReason::from(HoldReason::WorkDeposit);
            let (credit, remainder) =
                <T as Config>::Currency::slash(&reason, &work.owner, T::WorkDeposit::get());
            ensure!(remainder.is_zero(), Error::<T>::InconsistentState);
            if <T as Config>::Currency::resolve(&<T as Config>::RewardPool::get(), credit).is_err()
            {
                return Err(Error::<T>::InconsistentState.into());
            }
            work.status = WorkStatus::Failed;
            Works::<T>::insert(work_id, &*work);
            Self::remove_pending(work_id);
            Self::deposit_event(Event::WorkFailed { work_id });
            Ok(())
        }

        fn remove_pending(work_id: WorkId) {
            PendingWorks::<T>::mutate(|pending| {
                if let Some(index) = pending.iter().position(|id| *id == work_id) {
                    pending.swap_remove(index);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests;
