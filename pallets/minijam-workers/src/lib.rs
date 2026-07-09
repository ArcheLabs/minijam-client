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
    #[pallet::getter(fn current_epoch)]
    pub type CurrentEpoch<T> = StorageValue<_, EpochIndex, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn active_workers)]
    pub type ActiveWorkers<T: Config> =
        StorageValue<_, BoundedVec<WorkerId, T::TopWorkers>, ValueQuery>;

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
    }

    #[pallet::error]
    pub enum Error<T> {
        AlreadyRegistered,
        CandidatePoolFull,
        StakeBelowMinimum,
        WorkerIdOverflow,
        InvalidEpochLength,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(block: BlockNumberFor<T>) -> Weight {
            let epoch_length = T::EpochLength::get();
            if epoch_length == 0 {
                return T::DbWeight::get().reads(1);
            }
            let block_number: u64 = block.saturated_into();
            if block_number == 0 || !block_number.is_multiple_of(epoch_length as u64) {
                return T::DbWeight::get().reads(1);
            }

            let epoch = CurrentEpoch::<T>::get().saturating_add(1);
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

            T::DbWeight::get().reads_writes(u64::from(WorkerCount::<T>::get()).saturating_add(2), 2)
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
    }
}

extern crate alloc;

#[cfg(test)]
mod tests;
