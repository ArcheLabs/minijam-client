// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use alloc::{
        boxed::Box,
        collections::{BTreeMap, BTreeSet},
        vec::Vec,
    };
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
    use jam_codec::Decode as JamDecode;
    use jp_core_primitives::{
        simple::ByteSequence,
        state::StoreKey,
        work::{WorkPackage, WorkReport},
    };
    use minijam_jamcore_api::{
        ExecutionOutcome, MiniJamError, MiniJamExecutionInputV2, MiniJamExecutor,
        ProtocolStateReader, StateError,
    };
    use minijam_protocol::{
        blake2_256, CanonicalPreimageBytes, CanonicalReportBytes, ContentRef, Hash, PreimageBatch,
        PreimageMetadataV1, ProtocolStateChange, ReportEnvelopeV1, StateOperation, StateValue,
        SystemCommandV1, SystemOpBatch, SystemOpV1, PROTOCOL_VERSION_V1, PROTOCOL_VERSION_V2,
    };
    use minijam_state_adapter::{validate_execution_output_v2, ValidatedDelta, ValidationError};
    use pallet_minijam_workers::RoundDecision;
    use sp_runtime::traits::{One, SaturatedConversion, Saturating, Zero};

    pub type WorkId = u64;
    pub type BalanceOf<T> =
        <<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;
    const SYSTEM_SERVICE_ID: u32 = 0;
    const SYSTEM_STORAGE_RECEIPT_PREFIX: &[u8] = b"system/receipt/";

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    pub struct ServiceFuelAccount<Balance> {
        pub available: Balance,
        pub reserved: Balance,
    }

    impl<Balance: Default> Default for ServiceFuelAccount<Balance> {
        fn default() -> Self {
            Self {
                available: Default::default(),
                reserved: Default::default(),
            }
        }
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    pub struct ServiceFuelReservation<Balance> {
        pub service_id: u32,
        pub refine_limit: u64,
        pub accumulate_limit: u64,
        pub reserved: Balance,
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    pub struct WorkFuelSettlement<Balance> {
        pub charged: Balance,
        pub refunded: Balance,
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
    pub enum WorkStatus {
        InsufficientWorkers,
        AwaitingCandidate,
        Voting,
        Accepted,
        Imported,
        Failed,
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    #[scale_info(skip_type_params(T))]
    pub struct ExecutionItem<T: Config> {
        pub work_id: WorkId,
        pub execute_at: BlockNumberFor<T>,
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    #[scale_info(skip_type_params(T))]
    pub struct WorkRecord<T: Config> {
        pub owner: T::AccountId,
        pub package_hash: Hash,
        pub canonical_work_package: BoundedVec<u8, T::MaxWorkPackageBytes>,
        pub bundle_ref: ContentRef,
        pub fuel_reservation:
            BoundedVec<ServiceFuelReservation<BalanceOf<T>>, T::MaxServicesPerWork>,
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

    #[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    pub struct PreimageKeyV1 {
        pub requester: u32,
        pub blob_hash: Hash,
        pub blob_len: u32,
    }

    impl From<PreimageMetadataV1> for PreimageKeyV1 {
        fn from(metadata: PreimageMetadataV1) -> Self {
            Self {
                requester: metadata.requester,
                blob_hash: metadata.blob_hash,
                blob_len: metadata.blob_len,
            }
        }
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    #[scale_info(skip_type_params(T))]
    pub struct PendingPreimage<T: Config> {
        pub submitter: T::AccountId,
        pub canonical: CanonicalPreimageBytes,
        pub metadata: PreimageMetadataV1,
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    #[scale_info(skip_type_params(T))]
    pub struct PendingSystemOp<T: Config> {
        pub submitter: T::AccountId,
        pub op: SystemOpV1,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct BlockStfSummary {
        report_count: u32,
        preimage_count: u32,
        system_op_count: u32,
        receipt_hash: Hash,
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
        type FuelEscrowAccount: Get<Self::AccountId>;

        #[pallet::constant]
        type RefineGasPrice: Get<BalanceOf<Self>>;

        #[pallet::constant]
        type AccumulateGasPrice: Get<BalanceOf<Self>>;

        #[pallet::constant]
        type ReportSubmissionDeadline: Get<u32>;

        #[pallet::constant]
        type VoteWindow: Get<u32>;

        #[pallet::constant]
        type MaxCandidateRounds: Get<u8>;

        #[pallet::constant]
        type MaxPendingWorks: Get<u32>;

        #[pallet::constant]
        type MaxExecutionReports: Get<u32>;

        #[pallet::constant]
        type MaxExecutionGas: Get<u64>;

        #[pallet::constant]
        type MaxWorkPackageBytes: Get<u32>;

        #[pallet::constant]
        type MaxBundleBytes: Get<u64>;

        #[pallet::constant]
        type MaxServicesPerWork: Get<u32>;

        type JamCoreExecutor: MiniJamExecutor + Default;

        #[pallet::constant]
        type MaxPendingPreimages: Get<u32>;

        #[pallet::constant]
        type MaxPendingSystemOps: Get<u32>;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type NextWorkId<T> = StorageValue<_, WorkId, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn work)]
    pub type Works<T: Config> = StorageMap<_, Blake2_128Concat, WorkId, WorkRecord<T>, OptionQuery>;

    #[pallet::storage]
    pub type WorkByPackageHash<T: Config> =
        StorageMap<_, Blake2_128Concat, Hash, WorkId, OptionQuery>;

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
        StorageValue<_, BoundedVec<ExecutionItem<T>, T::MaxPendingWorks>, ValueQuery>;

    #[pallet::storage]
    pub type QuarantinedExecutionQueue<T: Config> =
        StorageValue<_, BoundedVec<ExecutionItem<T>, T::MaxPendingWorks>, ValueQuery>;

    #[pallet::storage]
    pub type ProtocolState<T: Config> =
        StorageMap<_, Blake2_128Concat, [u8; 31], StateValue, OptionQuery>;

    #[pallet::storage]
    pub type ExecutionReceipts<T: Config> =
        StorageMap<_, Blake2_128Concat, WorkId, Hash, OptionQuery>;

    #[pallet::storage]
    pub type LastExecutionReceipt<T: Config> = StorageValue<_, Hash, OptionQuery>;

    #[pallet::storage]
    pub type ReportImportPaused<T: Config> = StorageValue<_, bool, ValueQuery>;

    #[pallet::storage]
    pub type PreimageImportPaused<T: Config> = StorageValue<_, bool, ValueQuery>;

    #[pallet::storage]
    pub type SystemOpsPaused<T: Config> = StorageValue<_, bool, ValueQuery>;

    #[pallet::storage]
    pub type PendingPreimages<T: Config> =
        StorageValue<_, BoundedVec<PendingPreimage<T>, T::MaxPendingPreimages>, ValueQuery>;

    #[pallet::storage]
    pub type PendingPreimageKeys<T: Config> =
        StorageMap<_, Blake2_128Concat, PreimageKeyV1, (), OptionQuery>;

    #[pallet::storage]
    pub type PendingSystemOps<T: Config> =
        StorageValue<_, BoundedVec<PendingSystemOp<T>, T::MaxPendingSystemOps>, ValueQuery>;

    #[pallet::storage]
    pub type QuarantinedSystemOps<T: Config> =
        StorageValue<_, BoundedVec<PendingSystemOp<T>, T::MaxPendingSystemOps>, ValueQuery>;

    #[pallet::storage]
    pub type PendingSystemOpKeys<T: Config> =
        StorageMap<_, Blake2_128Concat, Hash, (), OptionQuery>;

    #[pallet::storage]
    pub type SystemOpNonces<T: Config> = StorageMap<_, Blake2_128Concat, [u8; 32], u64, ValueQuery>;

    #[pallet::storage]
    pub type ServiceFuelAccounts<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, ServiceFuelAccount<BalanceOf<T>>, ValueQuery>;

    #[pallet::storage]
    pub type TotalServiceFuel<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn work_fuel_settlement)]
    pub type WorkFuelSettlements<T: Config> =
        StorageMap<_, Blake2_128Concat, WorkId, WorkFuelSettlement<BalanceOf<T>>, OptionQuery>;

    #[pallet::genesis_config]
    #[derive(frame_support::DefaultNoBound)]
    pub struct GenesisConfig<T: Config> {
        pub protocol_state: Vec<(Vec<u8>, Vec<u8>)>,
        pub service_fuel: Vec<(u32, BalanceOf<T>)>,
        #[serde(skip)]
        pub _phantom: core::marker::PhantomData<T>,
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            for (key, value) in &self.protocol_state {
                let key: [u8; 31] = key
                    .as_slice()
                    .try_into()
                    .expect("MiniJAM genesis protocol-state keys must be 31 bytes");
                let value = StateValue::try_from(value.clone())
                    .expect("MiniJAM genesis protocol-state values must fit StateValue");
                ProtocolState::<T>::insert(key, value);
            }
            let mut total = BalanceOf::<T>::zero();
            for (service_id, amount) in &self.service_fuel {
                if amount.is_zero() {
                    continue;
                }
                ServiceFuelAccounts::<T>::mutate(service_id, |account| {
                    account.available = account.available.saturating_add(*amount);
                });
                total = total.saturating_add(*amount);
            }
            TotalServiceFuel::<T>::put(total);
        }
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        WorkSubmitted {
            work_id: WorkId,
            owner: T::AccountId,
            package_hash: Hash,
            bundle_ref: ContentRef,
            status: WorkStatus,
        },
        WorkFuelReserved {
            work_id: WorkId,
            total: BalanceOf<T>,
        },
        WorkFuelReleased {
            work_id: WorkId,
            total: BalanceOf<T>,
        },
        WorkFuelSettled {
            work_id: WorkId,
            charged: BalanceOf<T>,
            refunded: BalanceOf<T>,
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
        ReportImported {
            work_id: WorkId,
            receipt_hash: Hash,
        },
        PreimageQueued {
            requester: u32,
            blob_hash: Hash,
            blob_len: u32,
        },
        SystemOpQueued {
            request_id: Hash,
            sender: [u8; 32],
        },
        SystemOpConsumed {
            request_id: Hash,
        },
        SystemOpFailed {
            request_id: Hash,
            outcome: ExecutionOutcome,
        },
        SystemOpDropped {
            request_id: Hash,
        },
        SystemOpRetried {
            request_id: Hash,
        },
        SystemOpQuarantineCleared {
            count: u32,
        },
        ServiceFunded {
            funder: T::AccountId,
            service_id: u32,
            amount: BalanceOf<T>,
            new_available: BalanceOf<T>,
        },
        ExecutionYielded {
            work_id: WorkId,
            outcome: ExecutionOutcome,
        },
        ImportPaused {
            paused: bool,
        },
        BlockStfExecuted {
            slot: u32,
            report_count: u32,
            preimage_count: u32,
            system_op_count: u32,
            receipt_hash: Hash,
        },
        ExecutionQueueQuarantined {
            count: u32,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        WorkIdOverflow,
        TooManyPendingWorks,
        InvalidWorkPackage,
        DuplicateWorkPackage,
        InvalidContentRef,
        TooManyServicesPerWork,
        InsufficientServiceFuel,
        WorkNotFound,
        CandidateNotExpected,
        CandidateAlreadySubmitted,
        CandidateDeadlineExpired,
        InvalidEnvelope,
        InvalidReportHash,
        InvalidReportProjection,
        VotingSetupFailed,
        InconsistentState,
        ExecutionQueueFull,
        InvalidPreimage,
        DuplicatePendingPreimage,
        TooManyPendingPreimages,
        InvalidSystemOp,
        DuplicatePendingSystemOp,
        TooManyPendingSystemOps,
        QuarantinedSystemOpNotFound,
        UnknownService,
        ZeroFuelAmount,
        FuelEscrowInvariant,
        FuelSettlementInvariant,
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

        fn on_finalize(block: BlockNumberFor<T>) {
            Self::execute_block_stf(block);
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::DbWeight::get().reads_writes(6, 8))]
        #[transactional]
        pub fn submit_work(
            origin: OriginFor<T>,
            canonical_work_package: BoundedVec<u8, T::MaxWorkPackageBytes>,
            bundle_ref: ContentRef,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            ensure!(
                !canonical_work_package.is_empty(),
                Error::<T>::InvalidWorkPackage
            );
            Self::validate_content_ref(&bundle_ref)?;
            let work_package = Self::decode_work_package(&canonical_work_package)?;
            let fuel_reservation = Self::reserve_work_fuel(&work_package)?;
            let package_hash = blake2_256(&canonical_work_package);
            ensure!(
                !WorkByPackageHash::<T>::contains_key(package_hash),
                Error::<T>::DuplicateWorkPackage
            );
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
                    package_hash,
                    canonical_work_package,
                    bundle_ref: bundle_ref.clone(),
                    fuel_reservation: fuel_reservation.clone(),
                    round: 0,
                    status: WorkStatus::InsufficientWorkers,
                    candidate_deadline: Zero::zero(),
                },
            );
            WorkByPackageHash::<T>::insert(package_hash, work_id);
            NextWorkId::<T>::put(next);
            let _ = Self::prepare_round(work_id);
            let status = Works::<T>::get(work_id)
                .ok_or(Error::<T>::InconsistentState)?
                .status;
            Self::deposit_event(Event::WorkSubmitted {
                work_id,
                owner,
                package_hash,
                bundle_ref,
                status,
            });
            let total_reserved = fuel_reservation
                .iter()
                .fold(BalanceOf::<T>::zero(), |total, reservation| {
                    total.saturating_add(reservation.reserved)
                });
            if !total_reserved.is_zero() {
                Self::deposit_event(Event::WorkFuelReserved {
                    work_id,
                    total: total_reserved,
                });
            }
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
            Self::validate_candidate_report(&work, &envelope)?;

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

        #[pallet::call_index(2)]
        #[pallet::weight(T::DbWeight::get().writes(3))]
        pub fn pause_execution(origin: OriginFor<T>, paused: bool) -> DispatchResult {
            ensure_root(origin)?;
            ReportImportPaused::<T>::put(paused);
            PreimageImportPaused::<T>::put(paused);
            SystemOpsPaused::<T>::put(paused);
            Self::deposit_event(Event::ImportPaused { paused });
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(T::DbWeight::get().reads_writes(2, 2))]
        pub fn quarantine_pending(origin: OriginFor<T>) -> DispatchResult {
            ensure_root(origin)?;
            let queue = ExecutionQueue::<T>::take();
            let count = queue.len() as u32;
            QuarantinedExecutionQueue::<T>::put(queue);
            Self::deposit_event(Event::ExecutionQueueQuarantined { count });
            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(T::DbWeight::get().reads_writes(2, 2))]
        #[transactional]
        pub fn submit_preimage(
            origin: OriginFor<T>,
            canonical_preimage: CanonicalPreimageBytes,
        ) -> DispatchResult {
            let submitter = ensure_signed(origin)?;
            let state = FrameProtocolState::<T>(Default::default());
            let executor = T::JamCoreExecutor::default();
            let metadata = executor
                .validate_preimage_submission(&canonical_preimage, &state)
                .map_err(|_| Error::<T>::InvalidPreimage)?;
            let key = PreimageKeyV1::from(metadata);
            ensure!(
                !PendingPreimageKeys::<T>::contains_key(key),
                Error::<T>::DuplicatePendingPreimage
            );

            PendingPreimages::<T>::try_mutate(|pending| {
                pending
                    .try_push(PendingPreimage::<T> {
                        submitter,
                        canonical: canonical_preimage,
                        metadata,
                    })
                    .map_err(|_| Error::<T>::TooManyPendingPreimages)
            })?;
            PendingPreimageKeys::<T>::insert(key, ());
            Self::deposit_event(Event::PreimageQueued {
                requester: metadata.requester,
                blob_hash: metadata.blob_hash,
                blob_len: metadata.blob_len,
            });
            Ok(())
        }

        #[pallet::call_index(5)]
        #[pallet::weight(T::DbWeight::get().reads_writes(3, 4))]
        #[transactional]
        pub fn submit_system_op(
            origin: OriginFor<T>,
            command: Box<SystemCommandV1>,
        ) -> DispatchResult {
            let submitter = ensure_signed(origin)?;
            Self::validate_system_command(&command)?;
            let sender = Self::system_op_sender(&submitter);
            let nonce = SystemOpNonces::<T>::get(sender);
            let op = SystemOpV1::new(sender, nonce, *command);
            ensure!(
                !PendingSystemOpKeys::<T>::contains_key(op.request_id),
                Error::<T>::DuplicatePendingSystemOp
            );
            PendingSystemOps::<T>::try_mutate(|pending| {
                pending
                    .try_push(PendingSystemOp::<T> {
                        submitter,
                        op: op.clone(),
                    })
                    .map_err(|_| Error::<T>::TooManyPendingSystemOps)
            })?;
            PendingSystemOpKeys::<T>::insert(op.request_id, ());
            SystemOpNonces::<T>::insert(sender, nonce.saturating_add(1));
            Self::deposit_event(Event::SystemOpQueued {
                request_id: op.request_id,
                sender,
            });
            Ok(())
        }

        #[pallet::call_index(6)]
        #[pallet::weight(T::DbWeight::get().reads_writes(4, 3))]
        #[transactional]
        pub fn fund_service(
            origin: OriginFor<T>,
            service_id: u32,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            let funder = ensure_signed(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::ZeroFuelAmount);
            ensure!(Self::service_exists(service_id), Error::<T>::UnknownService);

            <T as Config>::Currency::transfer(
                &funder,
                &<T as Config>::FuelEscrowAccount::get(),
                amount,
                Preservation::Preserve,
            )?;

            let mut account = ServiceFuelAccounts::<T>::get(service_id);
            account.available = account.available.saturating_add(amount);
            ServiceFuelAccounts::<T>::insert(service_id, &account);
            let total = TotalServiceFuel::<T>::get().saturating_add(amount);
            TotalServiceFuel::<T>::put(total);
            ensure!(
                <T as Config>::Currency::balance(&<T as Config>::FuelEscrowAccount::get()) >= total,
                Error::<T>::FuelEscrowInvariant
            );

            Self::deposit_event(Event::ServiceFunded {
                funder,
                service_id,
                amount,
                new_available: account.available,
            });
            Ok(())
        }

        #[pallet::call_index(7)]
        #[pallet::weight(T::DbWeight::get().reads_writes(2, 2))]
        pub fn drop_quarantined_system_op(
            origin: OriginFor<T>,
            request_id: Hash,
        ) -> DispatchResult {
            ensure_root(origin)?;
            let mut removed = false;
            QuarantinedSystemOps::<T>::mutate(|ops| {
                if let Some(index) = ops
                    .iter()
                    .position(|pending| pending.op.request_id == request_id)
                {
                    ops.swap_remove(index);
                    removed = true;
                }
            });
            ensure!(removed, Error::<T>::QuarantinedSystemOpNotFound);
            Self::deposit_event(Event::SystemOpDropped { request_id });
            Ok(())
        }

        #[pallet::call_index(8)]
        #[pallet::weight(T::DbWeight::get().reads_writes(4, 4))]
        #[transactional]
        pub fn retry_quarantined_system_op(
            origin: OriginFor<T>,
            request_id: Hash,
        ) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(
                !PendingSystemOpKeys::<T>::contains_key(request_id),
                Error::<T>::DuplicatePendingSystemOp
            );

            let mut retry = None;
            QuarantinedSystemOps::<T>::mutate(|ops| {
                if let Some(index) = ops
                    .iter()
                    .position(|pending| pending.op.request_id == request_id)
                {
                    retry = Some(ops.swap_remove(index));
                }
            });
            let retry = retry.ok_or(Error::<T>::QuarantinedSystemOpNotFound)?;

            PendingSystemOps::<T>::try_mutate(|pending| {
                pending
                    .try_push(retry)
                    .map_err(|_| Error::<T>::TooManyPendingSystemOps)
            })?;
            PendingSystemOpKeys::<T>::insert(request_id, ());
            Self::deposit_event(Event::SystemOpRetried { request_id });
            Ok(())
        }

        #[pallet::call_index(9)]
        #[pallet::weight(T::DbWeight::get().reads_writes(1, 1))]
        pub fn clear_quarantined_system_ops(origin: OriginFor<T>) -> DispatchResult {
            ensure_root(origin)?;
            let count = QuarantinedSystemOps::<T>::take().len() as u32;
            Self::deposit_event(Event::SystemOpQuarantineCleared { count });
            Ok(())
        }
    }

    #[pallet::view_functions]
    impl<T: Config> Pallet<T> {
        pub fn get_work(work_id: WorkId) -> Option<WorkRecord<T>> {
            Works::<T>::get(work_id)
        }

        pub fn get_work_by_package_hash(package_hash: Hash) -> Option<WorkRecord<T>> {
            let work_id = WorkByPackageHash::<T>::get(package_hash)?;
            Works::<T>::get(work_id)
        }

        pub fn get_work_bundle_ref(work_id: WorkId) -> Option<ContentRef> {
            Works::<T>::get(work_id).map(|work| work.bundle_ref)
        }

        pub fn get_candidate(work_id: WorkId, round: u8) -> Option<CandidateRecord<T>> {
            Candidates::<T>::get(work_id, round)
        }

        pub fn get_execution_receipt(work_id: WorkId) -> Option<Hash> {
            ExecutionReceipts::<T>::get(work_id)
        }

        pub fn get_last_execution_receipt() -> Option<Hash> {
            LastExecutionReceipt::<T>::get()
        }

        pub fn get_service_fuel(service_id: u32) -> ServiceFuelAccount<BalanceOf<T>> {
            ServiceFuelAccounts::<T>::get(service_id)
        }

        pub fn get_work_fuel_reservation(
            work_id: WorkId,
        ) -> Option<BoundedVec<ServiceFuelReservation<BalanceOf<T>>, T::MaxServicesPerWork>>
        {
            Works::<T>::get(work_id).map(|work| work.fuel_reservation)
        }

        pub fn get_work_fuel_settlement(
            work_id: WorkId,
        ) -> Option<WorkFuelSettlement<BalanceOf<T>>> {
            WorkFuelSettlements::<T>::get(work_id)
        }

        pub fn get_pending_preimages() -> BoundedVec<PendingPreimage<T>, T::MaxPendingPreimages> {
            PendingPreimages::<T>::get()
        }

        pub fn has_pending_preimage(requester: u32, blob_hash: Hash, blob_len: u32) -> bool {
            PendingPreimageKeys::<T>::contains_key(PreimageKeyV1 {
                requester,
                blob_hash,
                blob_len,
            })
        }

        pub fn get_pending_system_ops() -> BoundedVec<PendingSystemOp<T>, T::MaxPendingSystemOps> {
            PendingSystemOps::<T>::get()
        }

        pub fn get_quarantined_system_ops() -> BoundedVec<PendingSystemOp<T>, T::MaxPendingSystemOps>
        {
            QuarantinedSystemOps::<T>::get()
        }

        pub fn get_system_op(request_id: Hash) -> Option<SystemOpV1> {
            PendingSystemOps::<T>::get()
                .into_iter()
                .find(|pending| pending.op.request_id == request_id)
                .map(|pending| pending.op)
        }

        pub fn get_system_receipt(request_id: Hash) -> Option<StateValue> {
            ProtocolState::<T>::get(Self::system_receipt_state_key(&request_id))
        }

        pub fn get_system_service_info() -> Option<StateValue> {
            ProtocolState::<T>::get(Self::service_info_state_key(0))
        }

        pub fn get_protocol_state(key: [u8; 31]) -> Option<StateValue> {
            ProtocolState::<T>::get(key)
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
            Self::validate_candidate_report(&work, &candidate.envelope)?;
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
            let execute_at = frame_system::Pallet::<T>::block_number().saturating_add(One::one());
            ExecutionQueue::<T>::try_mutate(|queue| {
                queue
                    .try_push(ExecutionItem::<T> {
                        work_id,
                        execute_at,
                    })
                    .map_err(|_| Error::<T>::ExecutionQueueFull)
            })?;
            Self::remove_pending(work_id);
            Self::deposit_event(Event::CandidateAccepted {
                work_id,
                round: work.round,
            });
            Ok(())
        }

        fn fail_work(work_id: WorkId, work: &mut WorkRecord<T>) -> DispatchResult {
            Self::release_work_fuel(work_id, work)?;
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

        fn execute_block_stf(block: BlockNumberFor<T>) {
            let mut queue = ExecutionQueue::<T>::get();
            let max_reports = T::MaxExecutionReports::get() as usize;
            let reports_paused = ReportImportPaused::<T>::get();
            let mut due: Vec<ExecutionItem<T>> = Vec::new();
            let mut retained: Vec<ExecutionItem<T>> = Vec::new();

            for item in queue.drain(..) {
                if !reports_paused && item.execute_at <= block && due.len() < max_reports {
                    due.push(item);
                } else {
                    retained.push(item);
                }
            }

            due.sort_by_key(|item| {
                let report_hash = Self::candidate_for_work(item.work_id)
                    .map(|(_, candidate)| candidate.envelope.canonical_report_hash)
                    .unwrap_or([0xff; 32]);
                (item.work_id, report_hash)
            });

            let result = with_transaction(|| match Self::execute_block_stf_inner(block, &due) {
                Ok(output) => {
                    let bounded = BoundedVec::<ExecutionItem<T>, T::MaxPendingWorks>::try_from(
                        retained.clone(),
                    )
                    .unwrap_or_else(|_| {
                        panic!("retained execution queue exceeded its original bound")
                    });
                    ExecutionQueue::<T>::put(bounded);
                    TransactionOutcome::Commit(Ok(output))
                }
                Err(error) => TransactionOutcome::Rollback(Err(error)),
            });

            match result {
                Ok(summary) => {
                    Self::deposit_event(Event::BlockStfExecuted {
                        slot: block.saturated_into(),
                        report_count: summary.report_count,
                        preimage_count: summary.preimage_count,
                        system_op_count: summary.system_op_count,
                        receipt_hash: summary.receipt_hash,
                    });
                }
                Err(ExecutionFailure::Yielded(work_id, outcome)) => {
                    Self::deposit_event(Event::ExecutionYielded { work_id, outcome });
                }
                Err(ExecutionFailure::SystemOpsYielded(outcome)) => {
                    Self::quarantine_pending_system_ops(outcome);
                }
                Err(ExecutionFailure::Fatal) => {
                    panic!("fatal MiniJam execution error");
                }
            }
        }

        fn execute_block_stf_inner(
            block: BlockNumberFor<T>,
            due: &[ExecutionItem<T>],
        ) -> Result<BlockStfSummary, ExecutionFailure> {
            let mut reports = Vec::<CanonicalReportBytes>::new();
            let mut work_ids = Vec::<WorkId>::new();
            for item in due {
                let Some((round, candidate)) = Self::candidate_for_work(item.work_id) else {
                    return Err(ExecutionFailure::Fatal);
                };
                let Some(work) = Works::<T>::get(item.work_id) else {
                    return Err(ExecutionFailure::Fatal);
                };
                if work.round != round || work.status != WorkStatus::Accepted {
                    return Err(ExecutionFailure::Fatal);
                }
                reports.push(candidate.envelope.canonical_report.clone());
                work_ids.push(item.work_id);
            }

            let reports: minijam_protocol::ReportBatch =
                reports.try_into().map_err(|_| ExecutionFailure::Fatal)?;
            let preimages = if PreimageImportPaused::<T>::get() {
                PreimageBatch::default()
            } else {
                Self::pending_preimage_batch()?
            };
            let system_ops = if SystemOpsPaused::<T>::get() {
                SystemOpBatch::default()
            } else {
                Self::pending_system_ops_batch()?
            };
            let report_count = reports.len() as u32;
            let preimage_count = preimages.len() as u32;
            let system_op_count = system_ops.len() as u32;
            let input = MiniJamExecutionInputV2 {
                protocol_version: PROTOCOL_VERSION_V2,
                slot: block.saturated_into(),
                parent_hash: Self::host_parent_hash(block),
                parent_state_root: Self::host_parent_state_root(block),
                entropy: Self::host_entropy(block),
                reports,
                preimages,
                system_ops,
                max_gas: T::MaxExecutionGas::get(),
            };
            let state = FrameProtocolState::<T>(Default::default());
            let executor = T::JamCoreExecutor::default();
            let output = match executor.execute_v2(input.clone(), &state) {
                Ok(output) => output,
                Err(MiniJamError::Execution(outcome)) => {
                    return if let Some(work_id) = work_ids.first().copied() {
                        Err(ExecutionFailure::Yielded(work_id, outcome))
                    } else if !input.system_ops.is_empty() {
                        Err(ExecutionFailure::SystemOpsYielded(outcome))
                    } else {
                        Err(ExecutionFailure::Fatal)
                    };
                }
                Err(MiniJamError::State(_)) | Err(MiniJamError::Invariant(_)) => {
                    return Err(ExecutionFailure::Fatal);
                }
                Err(MiniJamError::Input(_)) => return Err(ExecutionFailure::Fatal),
            };

            let delta = validate_execution_output_v2(&input, &output, &state)
                .map_err(Self::map_validation_error)?;
            Self::apply_delta(delta)?;
            Self::consume_preimages(&output.consumed_preimages);
            Self::consume_system_ops(&output.consumed_system_ops);

            let consumed_reports: BTreeSet<Hash> =
                output.consumed_reports.iter().copied().collect();
            for work_id in work_ids {
                let (_, candidate) =
                    Self::candidate_for_work(work_id).ok_or(ExecutionFailure::Fatal)?;
                if !consumed_reports.contains(&candidate.envelope.canonical_report_hash) {
                    continue;
                }
                let mut work = Works::<T>::get(work_id).ok_or(ExecutionFailure::Fatal)?;
                Self::settle_imported_work_fuel(
                    work_id,
                    &mut work,
                    &candidate.envelope.canonical_report,
                )?;
                work.status = WorkStatus::Imported;
                Works::<T>::insert(work_id, work);
                ExecutionReceipts::<T>::insert(work_id, output.receipt_hash);
                Self::deposit_event(Event::ReportImported {
                    work_id,
                    receipt_hash: output.receipt_hash,
                });
            }
            LastExecutionReceipt::<T>::put(output.receipt_hash);
            Ok(BlockStfSummary {
                report_count,
                preimage_count,
                system_op_count,
                receipt_hash: output.receipt_hash,
            })
        }

        fn map_validation_error(error: ValidationError) -> ExecutionFailure {
            match error {
                ValidationError::State(_) | ValidationError::Invariant(_) => {
                    ExecutionFailure::Fatal
                }
                ValidationError::GasExceeded | ValidationError::DeltaTooLarge => {
                    ExecutionFailure::Fatal
                }
            }
        }

        fn apply_delta(delta: ValidatedDelta) -> Result<(), ExecutionFailure> {
            for change in delta.into_changes() {
                Self::apply_change(change)?;
            }
            Ok(())
        }

        fn apply_change(change: ProtocolStateChange) -> Result<(), ExecutionFailure> {
            match change.operation {
                StateOperation::Upsert | StateOperation::Update => {
                    let value = change.value.ok_or(ExecutionFailure::Fatal)?;
                    ProtocolState::<T>::insert(change.key, value);
                }
                StateOperation::Remove => {
                    ProtocolState::<T>::remove(change.key);
                }
            }
            Ok(())
        }

        fn pending_preimage_batch() -> Result<PreimageBatch, ExecutionFailure> {
            let mut pending = PendingPreimages::<T>::get().into_inner();
            pending.sort_by_key(|preimage| {
                (
                    preimage.metadata.requester,
                    preimage.metadata.blob_hash,
                    preimage.metadata.blob_len,
                )
            });
            let preimages: Vec<CanonicalPreimageBytes> = pending
                .into_iter()
                .map(|preimage| preimage.canonical)
                .collect();
            preimages.try_into().map_err(|_| ExecutionFailure::Fatal)
        }

        fn pending_system_ops_batch() -> Result<SystemOpBatch, ExecutionFailure> {
            let mut pending = PendingSystemOps::<T>::get().into_inner();
            pending.sort_by_key(|pending| {
                (pending.op.sender, pending.op.nonce, pending.op.request_id)
            });
            let ops: Vec<SystemOpV1> = pending.into_iter().map(|pending| pending.op).collect();
            ops.try_into().map_err(|_| ExecutionFailure::Fatal)
        }

        fn consume_preimages(consumed_preimages: &[Hash]) {
            if consumed_preimages.is_empty() {
                return;
            }
            let consumed: BTreeSet<Hash> = consumed_preimages.iter().copied().collect();
            PendingPreimages::<T>::mutate(|pending| {
                let mut index = 0;
                while index < pending.len() {
                    let canonical_hash = blake2_256(&pending[index].canonical);
                    if consumed.contains(&canonical_hash) {
                        PendingPreimageKeys::<T>::remove(PreimageKeyV1::from(
                            pending[index].metadata,
                        ));
                        pending.swap_remove(index);
                    } else {
                        index += 1;
                    }
                }
            });
        }

        fn quarantine_pending_system_ops(outcome: ExecutionOutcome) {
            let pending = PendingSystemOps::<T>::take();
            if pending.is_empty() {
                return;
            }

            let mut quarantined = QuarantinedSystemOps::<T>::get();
            let mut overflowed = false;
            for pending in pending {
                PendingSystemOpKeys::<T>::remove(pending.op.request_id);
                Self::deposit_event(Event::SystemOpFailed {
                    request_id: pending.op.request_id,
                    outcome: outcome.clone(),
                });
                if quarantined.try_push(pending).is_err() {
                    overflowed = true;
                }
            }
            QuarantinedSystemOps::<T>::put(quarantined);
            if overflowed {
                SystemOpsPaused::<T>::put(true);
            }
        }

        fn consume_system_ops(consumed_system_ops: &[Hash]) {
            if consumed_system_ops.is_empty() {
                return;
            }
            let consumed: BTreeSet<Hash> = consumed_system_ops.iter().copied().collect();
            PendingSystemOps::<T>::mutate(|pending| {
                let mut index = 0;
                while index < pending.len() {
                    let request_id = pending[index].op.request_id;
                    if consumed.contains(&request_id) {
                        PendingSystemOpKeys::<T>::remove(request_id);
                        pending.swap_remove(index);
                        Self::deposit_event(Event::SystemOpConsumed { request_id });
                    } else {
                        index += 1;
                    }
                }
            });
        }

        fn validate_system_command(command: &SystemCommandV1) -> DispatchResult {
            match command {
                SystemCommandV1::CreateService {
                    code_len,
                    min_item_gas,
                    min_memo_gas,
                    initial_balance,
                    ..
                } => {
                    ensure!(*code_len > 0, Error::<T>::InvalidSystemOp);
                    ensure!(*min_item_gas > 0, Error::<T>::InvalidSystemOp);
                    ensure!(*min_memo_gas > 0, Error::<T>::InvalidSystemOp);
                    ensure!(*initial_balance > 0, Error::<T>::InvalidSystemOp);
                }
            }
            Ok(())
        }

        fn validate_content_ref(bundle_ref: &ContentRef) -> DispatchResult {
            ensure!(!bundle_ref.cid_v1.is_empty(), Error::<T>::InvalidContentRef);
            ensure!(bundle_ref.size > 0, Error::<T>::InvalidContentRef);
            ensure!(
                bundle_ref.size <= T::MaxBundleBytes::get(),
                Error::<T>::InvalidContentRef
            );
            Ok(())
        }

        fn decode_work_package(bytes: &[u8]) -> Result<WorkPackage, DispatchError> {
            let mut input = bytes;
            let package =
                WorkPackage::decode(&mut input).map_err(|_| Error::<T>::InvalidWorkPackage)?;
            ensure!(input.is_empty(), Error::<T>::InvalidWorkPackage);
            Ok(package)
        }

        fn validate_candidate_report(
            work: &WorkRecord<T>,
            envelope: &ReportEnvelopeV1,
        ) -> DispatchResult {
            let executor = T::JamCoreExecutor::default();
            let projection = executor
                .project_report(&envelope.canonical_report)
                .map_err(|_| Error::<T>::InvalidReportProjection)?;
            ensure!(
                projection.package_hash == work.package_hash,
                Error::<T>::InvalidReportProjection
            );
            ensure!(
                envelope.projected_metadata.package_hash == projection.package_hash
                    && envelope.projected_metadata.context_hash == projection.context_hash
                    && envelope.projected_metadata.exports_root == projection.exports_root
                    && envelope.projected_metadata.accumulate_gas
                        == projection.total_accumulate_gas,
                Error::<T>::InvalidReportProjection
            );

            let work_package = Self::decode_work_package(&work.canonical_work_package)?;
            ensure!(
                projection.result_count as usize == work_package.items.len()
                    && projection.services.len() == work_package.items.len(),
                Error::<T>::InvalidReportProjection
            );
            ensure!(
                projection.context_hash
                    == blake2_256(&jam_codec::Encode::encode(&work_package.context)),
                Error::<T>::InvalidReportProjection
            );
            let total_report_gas = projection
                .total_refine_gas
                .checked_add(projection.total_accumulate_gas)
                .ok_or(Error::<T>::InvalidReportProjection)?;
            ensure!(
                total_report_gas <= T::MaxExecutionGas::get(),
                Error::<T>::InvalidReportProjection
            );

            for (item, result) in work_package.items.iter().zip(projection.services.iter()) {
                ensure!(
                    result.service_id == item.service
                        && result.code_hash == item.code_hash.0
                        && result.refine_gas_used <= item.refine_gas_limit
                        && result.accumulate_gas <= item.accumulate_gas_limit,
                    Error::<T>::InvalidReportProjection
                );
            }
            Ok(())
        }

        fn decode_work_report(bytes: &[u8]) -> Result<WorkReport, ExecutionFailure> {
            let mut input = bytes;
            let report = WorkReport::decode(&mut input).map_err(|_| ExecutionFailure::Fatal)?;
            if !input.is_empty() {
                return Err(ExecutionFailure::Fatal);
            }
            Ok(report)
        }

        fn reserve_work_fuel(
            package: &WorkPackage,
        ) -> Result<
            BoundedVec<ServiceFuelReservation<BalanceOf<T>>, T::MaxServicesPerWork>,
            DispatchError,
        > {
            let mut grouped = BTreeMap::<u32, (u64, u64)>::new();
            for item in &package.items {
                let entry = grouped.entry(item.service).or_insert((0, 0));
                entry.0 = entry
                    .0
                    .checked_add(item.refine_gas_limit)
                    .ok_or(Error::<T>::InvalidWorkPackage)?;
                entry.1 = entry
                    .1
                    .checked_add(item.accumulate_gas_limit)
                    .ok_or(Error::<T>::InvalidWorkPackage)?;
            }

            let mut reservations = Vec::new();
            for (service_id, (refine_limit, accumulate_limit)) in grouped {
                ensure!(Self::service_exists(service_id), Error::<T>::UnknownService);
                let reserved = Self::fuel_cost(refine_limit, accumulate_limit);
                if reserved.is_zero() {
                    continue;
                }
                ServiceFuelAccounts::<T>::try_mutate(service_id, |account| {
                    ensure!(
                        account.available >= reserved,
                        Error::<T>::InsufficientServiceFuel
                    );
                    account.available = account.available.saturating_sub(reserved);
                    account.reserved = account.reserved.saturating_add(reserved);
                    Ok::<(), DispatchError>(())
                })?;
                reservations.push(ServiceFuelReservation {
                    service_id,
                    refine_limit,
                    accumulate_limit,
                    reserved,
                });
            }

            reservations
                .try_into()
                .map_err(|_| Error::<T>::TooManyServicesPerWork.into())
        }

        fn release_work_fuel(work_id: WorkId, work: &mut WorkRecord<T>) -> DispatchResult {
            if work.fuel_reservation.is_empty() {
                return Ok(());
            }

            let reservations = work.fuel_reservation.clone();
            let mut total_released = BalanceOf::<T>::zero();
            for reservation in &reservations {
                ServiceFuelAccounts::<T>::try_mutate(reservation.service_id, |account| {
                    ensure!(
                        account.reserved >= reservation.reserved,
                        Error::<T>::FuelSettlementInvariant
                    );
                    account.reserved = account.reserved.saturating_sub(reservation.reserved);
                    account.available = account.available.saturating_add(reservation.reserved);
                    Ok::<(), DispatchError>(())
                })?;
                total_released = total_released.saturating_add(reservation.reserved);
            }

            work.fuel_reservation = BoundedVec::default();
            WorkFuelSettlements::<T>::insert(
                work_id,
                WorkFuelSettlement {
                    charged: BalanceOf::<T>::zero(),
                    refunded: total_released,
                },
            );
            if !total_released.is_zero() {
                Self::deposit_event(Event::WorkFuelReleased {
                    work_id,
                    total: total_released,
                });
            }
            Ok(())
        }

        fn settle_imported_work_fuel(
            work_id: WorkId,
            work: &mut WorkRecord<T>,
            canonical_report: &[u8],
        ) -> Result<(), ExecutionFailure> {
            if work.fuel_reservation.is_empty() {
                return Ok(());
            }

            let report = Self::decode_work_report(canonical_report)?;
            if report.package_spec.hash.0 != work.package_hash {
                return Err(ExecutionFailure::Fatal);
            }

            let mut actual_by_service = BTreeMap::<u32, (u64, u64)>::new();
            for result in &report.results {
                let entry = actual_by_service.entry(result.service_id).or_insert((0, 0));
                entry.0 = entry
                    .0
                    .checked_add(result.refine_load.gas_used)
                    .ok_or(ExecutionFailure::Fatal)?;
                entry.1 = entry
                    .1
                    .checked_add(result.accumulate_gas)
                    .ok_or(ExecutionFailure::Fatal)?;
            }

            let reservations = work.fuel_reservation.clone();
            let mut charged_total = BalanceOf::<T>::zero();
            let mut refunded_total = BalanceOf::<T>::zero();
            for reservation in &reservations {
                let (refine_used, accumulate_used) = actual_by_service
                    .remove(&reservation.service_id)
                    .unwrap_or((0, 0));
                if refine_used > reservation.refine_limit
                    || accumulate_used > reservation.accumulate_limit
                {
                    return Err(ExecutionFailure::Fatal);
                }

                let charged = Self::fuel_cost(refine_used, accumulate_used);
                if charged > reservation.reserved {
                    return Err(ExecutionFailure::Fatal);
                }
                let refunded = reservation.reserved.saturating_sub(charged);

                ServiceFuelAccounts::<T>::try_mutate(reservation.service_id, |account| {
                    ensure!(
                        account.reserved >= reservation.reserved,
                        Error::<T>::FuelSettlementInvariant
                    );
                    account.reserved = account.reserved.saturating_sub(reservation.reserved);
                    account.available = account.available.saturating_add(refunded);
                    Ok::<(), DispatchError>(())
                })
                .map_err(|_| ExecutionFailure::Fatal)?;

                charged_total = charged_total.saturating_add(charged);
                refunded_total = refunded_total.saturating_add(refunded);
            }

            if !actual_by_service.is_empty() {
                return Err(ExecutionFailure::Fatal);
            }

            if !charged_total.is_zero() {
                let total_fuel = TotalServiceFuel::<T>::get();
                if total_fuel < charged_total {
                    return Err(ExecutionFailure::Fatal);
                }
                <T as Config>::Currency::transfer(
                    &<T as Config>::FuelEscrowAccount::get(),
                    &<T as Config>::RewardPool::get(),
                    charged_total,
                    Preservation::Expendable,
                )
                .map_err(|_| ExecutionFailure::Fatal)?;
                TotalServiceFuel::<T>::put(total_fuel.saturating_sub(charged_total));
            }

            work.fuel_reservation = BoundedVec::default();
            WorkFuelSettlements::<T>::insert(
                work_id,
                WorkFuelSettlement {
                    charged: charged_total,
                    refunded: refunded_total,
                },
            );
            Self::deposit_event(Event::WorkFuelSettled {
                work_id,
                charged: charged_total,
                refunded: refunded_total,
            });
            Ok(())
        }

        fn fuel_cost(refine_gas: u64, accumulate_gas: u64) -> BalanceOf<T> {
            let refine_cost = refine_gas
                .saturated_into::<BalanceOf<T>>()
                .saturating_mul(T::RefineGasPrice::get());
            let accumulate_cost = accumulate_gas
                .saturated_into::<BalanceOf<T>>()
                .saturating_mul(T::AccumulateGasPrice::get());
            refine_cost.saturating_add(accumulate_cost)
        }

        fn system_op_sender(account: &T::AccountId) -> [u8; 32] {
            blake2_256(&account.encode())
        }

        fn service_exists(service_id: u32) -> bool {
            ProtocolState::<T>::contains_key(Self::service_info_state_key(service_id))
        }

        fn system_receipt_state_key(request_id: &Hash) -> [u8; 31] {
            let mut storage_key = Vec::new();
            storage_key.extend_from_slice(SYSTEM_STORAGE_RECEIPT_PREFIX);
            storage_key.extend_from_slice(request_id);
            StoreKey::new_service_storage_key(&SYSTEM_SERVICE_ID, &ByteSequence::from(storage_key))
                .to_state_key()
                .0
        }

        fn service_info_state_key(service_id: u32) -> [u8; 31] {
            let service = service_id.to_le_bytes();
            let mut key = [0u8; 31];
            key[0] = 0xff;
            key[1] = service[0];
            key[3] = service[1];
            key[5] = service[2];
            key[7] = service[3];
            key
        }

        fn host_parent_hash(block: BlockNumberFor<T>) -> Hash {
            let parent_number = block.saturating_sub(One::one());
            let parent_hash = frame_system::Pallet::<T>::block_hash(parent_number);
            blake2_256(&parent_hash.encode())
        }

        fn host_parent_state_root(block: BlockNumberFor<T>) -> Hash {
            blake2_256(&(b"minijam/parent-state-root", Self::host_parent_hash(block)).encode())
        }

        fn host_entropy(block: BlockNumberFor<T>) -> Hash {
            blake2_256(&(b"minijam/host-entropy", block).encode())
        }

        fn candidate_for_work(work_id: WorkId) -> Option<(u8, CandidateRecord<T>)> {
            let work = Works::<T>::get(work_id)?;
            Candidates::<T>::get(work_id, work.round).map(|candidate| (work.round, candidate))
        }
    }

    pub struct FrameProtocolState<T: Config>(core::marker::PhantomData<T>);

    impl<T: Config> ProtocolStateReader for FrameProtocolState<T> {
        fn get(&self, key: &[u8; 31]) -> Result<Option<Vec<u8>>, StateError> {
            Ok(ProtocolState::<T>::get(key).map(|value| value.into_inner()))
        }
    }

    enum ExecutionFailure {
        Yielded(WorkId, ExecutionOutcome),
        SystemOpsYielded(ExecutionOutcome),
        Fatal,
    }

    impl From<DispatchError> for ExecutionFailure {
        fn from(_: DispatchError) -> Self {
            Self::Fatal
        }
    }
}

#[cfg(test)]
mod tests;
