// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{
        pallet_prelude::*,
        traits::tokens::fungible::{Inspect, MutateHold},
    };
    use frame_system::pallet_prelude::*;
    use minijam_protocol::{Verdict, WorkerVoteV1};
    use parity_scale_codec::Encode;
    use sp_core::sr25519;
    use sp_runtime::traits::SaturatedConversion;

    pub type WorkerId = u64;
    pub type EpochIndex = u32;
    pub type BalanceOf<T> =
        <<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    #[scale_info(skip_type_params(T))]
    pub struct WorkerRecord<T: Config> {
        pub owner: T::AccountId,
        pub session_key: [u8; 32],
        pub active_stake: BalanceOf<T>,
        pub effective_epoch: EpochIndex,
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    pub struct PendingWorkerUpdate<Balance> {
        pub session_key: Option<[u8; 32]>,
        pub stake: Option<Balance>,
        pub effective_epoch: EpochIndex,
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    pub struct UnbondingChunk<Balance> {
        pub amount: Balance,
        pub unlock_epoch: EpochIndex,
    }

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
    pub enum RoundDecision {
        Accepted,
        Rejected,
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    #[scale_info(skip_type_params(T))]
    pub struct VotingRound<T: Config> {
        pub assignment_epoch: EpochIndex,
        pub candidate_hash: [u8; 32],
        pub deadline: BlockNumberFor<T>,
        pub support: u32,
        pub oppose: u32,
        pub responses: u32,
        pub locked: Option<RoundDecision>,
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    #[scale_info(skip_type_params(T))]
    pub struct RoundResult<T: Config> {
        pub decision: Option<RoundDecision>,
        pub absentees: BoundedVec<WorkerId, T::WorkersPerWork>,
    }

    #[pallet::composite_enum]
    pub enum HoldReason {
        WorkerStake,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Currency: MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>;

        type RuntimeHoldReason: From<HoldReason>;

        #[pallet::constant]
        type MinimumStake: Get<BalanceOf<Self>>;

        #[pallet::constant]
        type EpochLength: Get<u32>;

        #[pallet::constant]
        type MaxCandidates: Get<u32>;

        #[pallet::constant]
        type TopWorkers: Get<u32>;

        #[pallet::constant]
        type AssignmentSeedDelay: Get<u32>;

        #[pallet::constant]
        type WorkersPerWork: Get<u32>;

        #[pallet::constant]
        type MaxWorksPerRound: Get<u32>;

        #[pallet::constant]
        type MaxDutiesPerWorkerPerRound: Get<u32>;

        #[pallet::constant]
        type SupportThreshold: Get<u32>;

        #[pallet::constant]
        type OpposeThreshold: Get<u32>;

        #[pallet::constant]
        type MaxOpenVotes: Get<u32>;

        #[pallet::constant]
        type ChainId: Get<[u8; 32]>;

        #[pallet::constant]
        type ProtocolVersion: Get<u16>;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type NextWorkerId<T> = StorageValue<_, WorkerId, ValueQuery>;

    #[pallet::storage]
    pub type WorkerCount<T> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    pub type WorkerByAccount<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, WorkerId, OptionQuery>;

    #[pallet::storage]
    pub type Workers<T: Config> =
        StorageMap<_, Blake2_128Concat, WorkerId, WorkerRecord<T>, OptionQuery>;

    #[pallet::storage]
    pub type PendingUpdates<T: Config> =
        StorageMap<_, Blake2_128Concat, WorkerId, PendingWorkerUpdate<BalanceOf<T>>, OptionQuery>;

    #[pallet::storage]
    pub type Unbonding<T: Config> =
        StorageMap<_, Blake2_128Concat, WorkerId, UnbondingChunk<BalanceOf<T>>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn current_epoch)]
    pub type CurrentEpoch<T> = StorageValue<_, EpochIndex, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn active_workers)]
    pub type ActiveWorkers<T: Config> =
        StorageValue<_, BoundedVec<WorkerId, T::TopWorkers>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn assignment)]
    pub type Assignments<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        u8,
        BoundedVec<WorkerId, T::WorkersPerWork>,
        OptionQuery,
    >;

    #[pallet::storage]
    pub type AssignedWorkCount<T> =
        StorageMap<_, Blake2_128Concat, (EpochIndex, u8), u32, ValueQuery>;

    #[pallet::storage]
    pub type DutyCounts<T> =
        StorageMap<_, Blake2_128Concat, (EpochIndex, u8, WorkerId), u32, ValueQuery>;

    #[pallet::storage]
    pub type VotingRounds<T: Config> =
        StorageMap<_, Blake2_128Concat, (u64, u8), VotingRound<T>, OptionQuery>;

    #[pallet::storage]
    pub type OpenVoteRounds<T: Config> =
        StorageValue<_, BoundedVec<(u64, u8), T::MaxOpenVotes>, ValueQuery>;

    #[pallet::storage]
    pub type Votes<T> = StorageMap<_, Blake2_128Concat, (u64, u8, WorkerId), Verdict, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn round_result)]
    pub type RoundResults<T: Config> =
        StorageMap<_, Blake2_128Concat, (u64, u8), RoundResult<T>, OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        WorkerRegistered {
            worker_id: WorkerId,
            owner: T::AccountId,
            effective_epoch: EpochIndex,
            stake: BalanceOf<T>,
        },
        EpochSnapshot {
            epoch: EpochIndex,
            workers: BoundedVec<WorkerId, T::TopWorkers>,
        },
        WorkerUpdateScheduled {
            worker_id: WorkerId,
            effective_epoch: EpochIndex,
        },
        WorkerUpdateApplied {
            worker_id: WorkerId,
            epoch: EpochIndex,
        },
        StakeReleased {
            worker_id: WorkerId,
            amount: BalanceOf<T>,
        },
        VoteSubmitted {
            work_id: u64,
            round: u8,
            worker_id: WorkerId,
        },
        VotingFinalized {
            work_id: u64,
            round: u8,
            decision: Option<RoundDecision>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        AlreadyRegistered,
        CandidatePoolFull,
        StakeBelowMinimum,
        WorkerIdOverflow,
        InvalidEpochLength,
        NotRegistered,
        EmptyUpdate,
        PendingUpdateExists,
        UnbondingInProgress,
        TooManyWorks,
        InsufficientWorkers,
        VotingAlreadyOpen,
        TooManyOpenVotes,
        VotingNotOpen,
        NotAssigned,
        VoteExpired,
        VoteMismatch,
        AlreadyVoted,
        InvalidSignature,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(block: BlockNumberFor<T>) -> Weight {
            Self::finalize_due_votes(block);
            let epoch_length = T::EpochLength::get();
            if epoch_length == 0 {
                return T::DbWeight::get().reads(1);
            }
            let block_number: u64 = block.saturated_into();
            if block_number == 0 || !block_number.is_multiple_of(epoch_length as u64) {
                return T::DbWeight::get().reads(1);
            }

            let epoch = CurrentEpoch::<T>::get().saturating_add(1);
            Self::apply_pending_updates(epoch);
            Self::release_mature_unbonding(epoch);
            let mut eligible: BoundedVec<(WorkerId, BalanceOf<T>), T::MaxCandidates> =
                BoundedVec::default();
            for (worker_id, worker) in Workers::<T>::iter() {
                if worker.effective_epoch <= epoch {
                    let _ = eligible.try_push((worker_id, worker.active_stake));
                }
            }
            eligible.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));

            let workers: BoundedVec<WorkerId, T::TopWorkers> = eligible
                .into_iter()
                .take(T::TopWorkers::get() as usize)
                .map(|(worker_id, _)| worker_id)
                .collect::<alloc::vec::Vec<_>>()
                .try_into()
                .expect("take is bounded by TopWorkers");

            CurrentEpoch::<T>::put(epoch);
            ActiveWorkers::<T>::put(&workers);
            Self::deposit_event(Event::EpochSnapshot { epoch, workers });

            let worker_count = u64::from(WorkerCount::<T>::get());
            T::DbWeight::get().reads_writes(
                worker_count.saturating_mul(3).saturating_add(2),
                worker_count.saturating_mul(2).saturating_add(2),
            )
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::DbWeight::get().reads_writes(4, 4))]
        pub fn register(
            origin: OriginFor<T>,
            session_key: [u8; 32],
            stake: BalanceOf<T>,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            ensure!(
                !WorkerByAccount::<T>::contains_key(&owner),
                Error::<T>::AlreadyRegistered
            );
            ensure!(
                WorkerCount::<T>::get() < T::MaxCandidates::get(),
                Error::<T>::CandidatePoolFull
            );
            ensure!(
                stake >= T::MinimumStake::get(),
                Error::<T>::StakeBelowMinimum
            );
            ensure!(T::EpochLength::get() > 0, Error::<T>::InvalidEpochLength);

            let worker_id = NextWorkerId::<T>::get();
            let next_worker_id = worker_id
                .checked_add(1)
                .ok_or(Error::<T>::WorkerIdOverflow)?;
            let reason = T::RuntimeHoldReason::from(HoldReason::WorkerStake);
            T::Currency::hold(&reason, &owner, stake)?;

            let effective_epoch = CurrentEpoch::<T>::get().saturating_add(1);
            Workers::<T>::insert(
                worker_id,
                WorkerRecord::<T> {
                    owner: owner.clone(),
                    session_key,
                    active_stake: stake,
                    effective_epoch,
                },
            );
            WorkerByAccount::<T>::insert(&owner, worker_id);
            NextWorkerId::<T>::put(next_worker_id);
            WorkerCount::<T>::mutate(|count| *count = count.saturating_add(1));
            Self::deposit_event(Event::WorkerRegistered {
                worker_id,
                owner,
                effective_epoch,
                stake,
            });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::DbWeight::get().reads_writes(5, 2))]
        pub fn schedule_update(
            origin: OriginFor<T>,
            session_key: Option<[u8; 32]>,
            stake: Option<BalanceOf<T>>,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            ensure!(
                session_key.is_some() || stake.is_some(),
                Error::<T>::EmptyUpdate
            );
            let worker_id = WorkerByAccount::<T>::get(&owner).ok_or(Error::<T>::NotRegistered)?;
            ensure!(
                !PendingUpdates::<T>::contains_key(worker_id),
                Error::<T>::PendingUpdateExists
            );
            ensure!(
                !Unbonding::<T>::contains_key(worker_id),
                Error::<T>::UnbondingInProgress
            );
            let worker = Workers::<T>::get(worker_id).ok_or(Error::<T>::NotRegistered)?;

            if let Some(new_stake) = stake {
                ensure!(
                    new_stake >= T::MinimumStake::get(),
                    Error::<T>::StakeBelowMinimum
                );
                if new_stake > worker.active_stake {
                    let reason = T::RuntimeHoldReason::from(HoldReason::WorkerStake);
                    T::Currency::hold(&reason, &owner, new_stake - worker.active_stake)?;
                }
            }

            let effective_epoch = CurrentEpoch::<T>::get().saturating_add(1);
            PendingUpdates::<T>::insert(
                worker_id,
                PendingWorkerUpdate {
                    session_key,
                    stake,
                    effective_epoch,
                },
            );
            Self::deposit_event(Event::WorkerUpdateScheduled {
                worker_id,
                effective_epoch,
            });
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::DbWeight::get().reads_writes(6, 5))]
        pub fn submit_vote(
            origin: OriginFor<T>,
            worker_id: WorkerId,
            vote: WorkerVoteV1,
            signature: [u8; 64],
        ) -> DispatchResult {
            let _relayer = ensure_signed(origin)?;
            let key = (vote.work_id, vote.round);
            let mut voting = VotingRounds::<T>::get(key).ok_or(Error::<T>::VotingNotOpen)?;
            let assignment =
                Assignments::<T>::get(vote.work_id, vote.round).ok_or(Error::<T>::NotAssigned)?;
            ensure!(assignment.contains(&worker_id), Error::<T>::NotAssigned);
            ensure!(
                !Votes::<T>::contains_key((vote.work_id, vote.round, worker_id)),
                Error::<T>::AlreadyVoted
            );
            ensure!(
                frame_system::Pallet::<T>::block_number() <= voting.deadline,
                Error::<T>::VoteExpired
            );
            ensure!(
                vote.assignment_epoch == voting.assignment_epoch
                    && vote.candidate_report_hash == voting.candidate_hash
                    && vote.deadline == voting.deadline.saturated_into::<u32>()
                    && vote.chain_id == T::ChainId::get()
                    && vote.protocol_version == T::ProtocolVersion::get(),
                Error::<T>::VoteMismatch
            );
            let worker = Workers::<T>::get(worker_id).ok_or(Error::<T>::NotRegistered)?;
            let valid = sp_io::crypto::sr25519_verify(
                &sr25519::Signature::from_raw(signature),
                &vote.signing_hash(),
                &sr25519::Public::from_raw(worker.session_key),
            );
            ensure!(valid, Error::<T>::InvalidSignature);

            match &vote.verdict {
                Verdict::Support => voting.support = voting.support.saturating_add(1),
                Verdict::Oppose(_) => voting.oppose = voting.oppose.saturating_add(1),
            }
            voting.responses = voting.responses.saturating_add(1);
            if voting.locked.is_none() {
                if voting.support >= T::SupportThreshold::get() {
                    voting.locked = Some(RoundDecision::Accepted);
                } else if voting.oppose >= T::OpposeThreshold::get() {
                    voting.locked = Some(RoundDecision::Rejected);
                }
            }
            Votes::<T>::insert((vote.work_id, vote.round, worker_id), vote.verdict);
            VotingRounds::<T>::insert(key, &voting);
            Self::deposit_event(Event::VoteSubmitted {
                work_id: vote.work_id,
                round: vote.round,
                worker_id,
            });
            if voting.responses >= assignment.len() as u32 {
                Self::finalize_voting(vote.work_id, vote.round)?;
            }
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn open_voting(
            work_id: u64,
            round: u8,
            candidate_hash: [u8; 32],
            deadline: BlockNumberFor<T>,
        ) -> DispatchResult {
            ensure!(
                Assignments::<T>::contains_key(work_id, round),
                Error::<T>::NotAssigned
            );
            ensure!(
                !VotingRounds::<T>::contains_key((work_id, round))
                    && !RoundResults::<T>::contains_key((work_id, round)),
                Error::<T>::VotingAlreadyOpen
            );
            ensure!(
                deadline > frame_system::Pallet::<T>::block_number(),
                Error::<T>::VoteExpired
            );
            OpenVoteRounds::<T>::try_mutate(|rounds| {
                rounds
                    .try_push((work_id, round))
                    .map_err(|_| Error::<T>::TooManyOpenVotes)
            })?;
            VotingRounds::<T>::insert(
                (work_id, round),
                VotingRound::<T> {
                    assignment_epoch: CurrentEpoch::<T>::get(),
                    candidate_hash,
                    deadline,
                    support: 0,
                    oppose: 0,
                    responses: 0,
                    locked: None,
                },
            );
            Ok(())
        }

        pub fn assign_work(
            work_id: u64,
            round: u8,
        ) -> Result<BoundedVec<WorkerId, T::WorkersPerWork>, DispatchError> {
            if let Some(existing) = Assignments::<T>::get(work_id, round) {
                return Ok(existing);
            }
            let epoch = CurrentEpoch::<T>::get();
            ensure!(
                AssignedWorkCount::<T>::get((epoch, round)) < T::MaxWorksPerRound::get(),
                Error::<T>::TooManyWorks
            );

            let epoch_start = u64::from(epoch).saturating_mul(u64::from(T::EpochLength::get()));
            let seed_height = epoch_start.saturating_sub(u64::from(T::AssignmentSeedDelay::get()));
            let seed_block: BlockNumberFor<T> = seed_height.saturated_into();
            let seed = frame_system::Pallet::<T>::block_hash(seed_block);

            let mut candidates = ActiveWorkers::<T>::get().into_inner();
            candidates.retain(|worker_id| {
                DutyCounts::<T>::get((epoch, round, *worker_id))
                    < T::MaxDutiesPerWorkerPerRound::get()
            });
            candidates.sort_by_key(|worker_id| {
                let duty = DutyCounts::<T>::get((epoch, round, *worker_id));
                let score = sp_io::hashing::blake2_256(
                    &(
                        b"minijam/assignment-v1".as_slice(),
                        seed,
                        epoch,
                        round,
                        work_id,
                        worker_id,
                    )
                        .encode(),
                );
                (duty, score, *worker_id)
            });

            ensure!(
                candidates.len() >= T::WorkersPerWork::get() as usize,
                Error::<T>::InsufficientWorkers
            );
            let assigned: BoundedVec<WorkerId, T::WorkersPerWork> = candidates
                .into_iter()
                .take(T::WorkersPerWork::get() as usize)
                .collect::<alloc::vec::Vec<_>>()
                .try_into()
                .expect("take is bounded by WorkersPerWork");
            for worker_id in &assigned {
                DutyCounts::<T>::mutate((epoch, round, *worker_id), |count| {
                    *count = count.saturating_add(1);
                });
            }
            AssignedWorkCount::<T>::mutate((epoch, round), |count| {
                *count = count.saturating_add(1);
            });
            Assignments::<T>::insert(work_id, round, &assigned);
            Ok(assigned)
        }

        fn apply_pending_updates(epoch: EpochIndex) {
            for (worker_id, update) in PendingUpdates::<T>::iter() {
                if update.effective_epoch > epoch {
                    continue;
                }
                Workers::<T>::mutate(worker_id, |maybe_worker| {
                    let Some(worker) = maybe_worker else {
                        return;
                    };
                    if let Some(session_key) = update.session_key {
                        worker.session_key = session_key;
                    }
                    if let Some(stake) = update.stake {
                        if stake < worker.active_stake {
                            Unbonding::<T>::insert(
                                worker_id,
                                UnbondingChunk {
                                    amount: worker.active_stake - stake,
                                    unlock_epoch: epoch.saturating_add(2),
                                },
                            );
                        }
                        worker.active_stake = stake;
                    }
                    worker.effective_epoch = epoch;
                });
                PendingUpdates::<T>::remove(worker_id);
                Self::deposit_event(Event::WorkerUpdateApplied { worker_id, epoch });
            }
        }

        fn release_mature_unbonding(epoch: EpochIndex) {
            for (worker_id, chunk) in Unbonding::<T>::iter() {
                if chunk.unlock_epoch > epoch {
                    continue;
                }
                let Some(worker) = Workers::<T>::get(worker_id) else {
                    continue;
                };
                let reason = T::RuntimeHoldReason::from(HoldReason::WorkerStake);
                if T::Currency::release(
                    &reason,
                    &worker.owner,
                    chunk.amount,
                    frame_support::traits::tokens::Precision::Exact,
                )
                .is_ok()
                {
                    Unbonding::<T>::remove(worker_id);
                    Self::deposit_event(Event::StakeReleased {
                        worker_id,
                        amount: chunk.amount,
                    });
                }
            }
        }

        fn finalize_due_votes(block: BlockNumberFor<T>) {
            let rounds = OpenVoteRounds::<T>::get();
            for (work_id, round) in rounds {
                if let Some(voting) = VotingRounds::<T>::get((work_id, round)) {
                    if block > voting.deadline {
                        let _ = Self::finalize_voting(work_id, round);
                    }
                }
            }
        }

        fn finalize_voting(work_id: u64, round: u8) -> DispatchResult {
            let voting =
                VotingRounds::<T>::take((work_id, round)).ok_or(Error::<T>::VotingNotOpen)?;
            let assignment =
                Assignments::<T>::get(work_id, round).ok_or(Error::<T>::NotAssigned)?;
            let absentees: BoundedVec<WorkerId, T::WorkersPerWork> = assignment
                .into_iter()
                .filter(|worker_id| !Votes::<T>::contains_key((work_id, round, *worker_id)))
                .collect::<alloc::vec::Vec<_>>()
                .try_into()
                .expect("absentees are bounded by assignment");
            RoundResults::<T>::insert(
                (work_id, round),
                RoundResult::<T> {
                    decision: voting.locked,
                    absentees,
                },
            );
            OpenVoteRounds::<T>::mutate(|rounds| {
                if let Some(index) = rounds.iter().position(|key| *key == (work_id, round)) {
                    rounds.swap_remove(index);
                }
            });
            Self::deposit_event(Event::VotingFinalized {
                work_id,
                round,
                decision: voting.locked,
            });
            Ok(())
        }
    }
}

extern crate alloc;

#[cfg(test)]
mod tests;
