// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

//! The MiniJAM consensus boundary.
//!
//! WorkPackages and bundles are Formal RPC artifacts. The only work value
//! crossing into consensus is a canonical WorkReport submitted by the
//! configured single Worker account.

extern crate alloc;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use alloc::{boxed::Box, collections::BTreeSet, vec::Vec};
    use frame_support::{
        pallet_prelude::*,
        storage::{with_transaction, TransactionOutcome},
        traits::tokens::fungible::Inspect,
        transactional,
    };
    use frame_system::pallet_prelude::*;
    use minijam_jamcore_api::{
        ExecutionOutcome, MiniJamError, MiniJamExecutionInput, MiniJamExecutor,
        ProtocolStateReader, StateError,
    };
    use minijam_protocol::{
        blake2_256, CanonicalPreimageBytes, CanonicalReportBytes, Hash, PackageStatus,
        PreimageBatch, PreimageMetadataV1, ProtocolStateChange, ReportBatch, StateOperation,
        StateValue, SystemCommandV2, SystemOpBatch, SystemOpV2, PROTOCOL_VERSION_V1,
    };
    use minijam_state_adapter::{validate_execution_output, ValidatedDelta, ValidationError};
    use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
    use scale_info::TypeInfo;
    use sp_runtime::traits::{One, SaturatedConversion, Saturating};

    const ALLOCATION_SYSTEM_SENDER: [u8; 32] = [0xa1; 32];
    const ALLOCATION_RECEIPT_PREFIX: &[u8] = b"system/allocation/";
    const SYSTEM_SERVICE_ID: u32 = 0;
    const SYSTEM_STORAGE_RECEIPT_PREFIX: &[u8] = b"system/receipt/";

    pub type BalanceOf<T> =
        <<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

    #[derive(
        Clone, Debug, Decode, DecodeWithMemTracking, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo,
    )]
    pub struct AllocationV1<Balance> {
        pub allocation_id: u64,
        pub target_service: u32,
        pub amount: Balance,
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    #[scale_info(skip_type_params(T))]
    pub struct PendingAllocation<T: Config> {
        pub submitter: T::AccountId,
        pub allocation: AllocationV1<BalanceOf<T>>,
    }

    #[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    pub enum AllocationStatus {
        Pending,
        Processed,
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    pub struct AllocationReceipt<Balance> {
        pub allocation: AllocationV1<Balance>,
        pub status: AllocationStatus,
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    pub struct PendingReport {
        pub package_hash: Hash,
        pub canonical_report: CanonicalReportBytes,
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
    pub enum ExecutionErrorCode {
        OutOfGas,
        Trap,
        ServiceFailure,
        InvalidInput,
        InvalidOutput,
        GasExceeded,
        DeltaTooLarge,
    }

    impl From<ExecutionOutcome> for ExecutionErrorCode {
        fn from(outcome: ExecutionOutcome) -> Self {
            match outcome {
                ExecutionOutcome::OutOfGas => Self::OutOfGas,
                ExecutionOutcome::Trap => Self::Trap,
                ExecutionOutcome::ServiceFailure => Self::ServiceFailure,
            }
        }
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
        pub op: SystemOpV2,
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    #[scale_info(skip_type_params(T))]
    pub struct QuarantinedSystemOp<T: Config> {
        pub submitter: T::AccountId,
        pub op: SystemOpV2,
        pub canonical_hash: Hash,
        pub error_code: ExecutionErrorCode,
        pub block_number: BlockNumberFor<T>,
        pub retryable: bool,
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    #[scale_info(skip_type_params(T))]
    pub struct QuarantinedPreimage<T: Config> {
        pub submitter: T::AccountId,
        pub metadata: PreimageMetadataV1,
        pub canonical_hash: Hash,
        pub error_code: ExecutionErrorCode,
        pub block_number: BlockNumberFor<T>,
        pub retryable: bool,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct BlockStfSummary {
        report_count: u32,
        preimage_count: u32,
        system_op_count: u32,
        receipt_hash: Hash,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type Currency: Inspect<Self::AccountId>;
        #[pallet::constant]
        type MaxPendingAllocations: Get<u32>;
        #[pallet::constant]
        type MaxExecutionReports: Get<u32>;
        #[pallet::constant]
        type MaxExecutionGas: Get<u64>;
        #[pallet::constant]
        type MaxPendingReports: Get<u32>;
        #[pallet::constant]
        type MaxPendingPreimages: Get<u32>;
        #[pallet::constant]
        type MaxPendingSystemOps: Get<u32>;
        #[pallet::constant]
        type ChainId: Get<Hash>;
        type JamCoreExecutor: MiniJamExecutor + Default;
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn worker_account)]
    pub type WorkerAccount<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;
    #[pallet::storage]
    #[pallet::getter(fn allocation_relayer)]
    pub type AllocationRelayer<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;
    #[pallet::storage]
    pub type PackageStatuses<T: Config> =
        StorageMap<_, Blake2_128Concat, Hash, PackageStatus, OptionQuery>;
    #[pallet::storage]
    pub type PendingReports<T: Config> =
        StorageValue<_, BoundedVec<PendingReport, T::MaxPendingReports>, ValueQuery>;
    #[pallet::storage]
    pub type PackageFailures<T: Config> =
        StorageMap<_, Blake2_128Concat, Hash, ExecutionErrorCode, OptionQuery>;
    #[pallet::storage]
    pub type ExecutionReceiptsByPackageHash<T: Config> =
        StorageMap<_, Blake2_128Concat, Hash, Hash, OptionQuery>;
    #[pallet::storage]
    pub type LastExecutionReceipt<T: Config> = StorageValue<_, Hash, OptionQuery>;
    #[pallet::storage]
    pub type ReportImportPaused<T: Config> = StorageValue<_, bool, ValueQuery>;
    #[pallet::storage]
    pub type PreimageImportPaused<T: Config> = StorageValue<_, bool, ValueQuery>;
    #[pallet::storage]
    pub type SystemOpsPaused<T: Config> = StorageValue<_, bool, ValueQuery>;
    #[pallet::storage]
    pub type ProtocolState<T: Config> =
        StorageMap<_, Blake2_128Concat, [u8; 31], StateValue, OptionQuery>;
    #[pallet::storage]
    pub type PendingPreimages<T: Config> =
        StorageValue<_, BoundedVec<PendingPreimage<T>, T::MaxPendingPreimages>, ValueQuery>;
    #[pallet::storage]
    pub type PendingPreimageKeys<T: Config> =
        StorageMap<_, Blake2_128Concat, PreimageKeyV1, (), OptionQuery>;
    #[pallet::storage]
    pub type QuarantinedPreimages<T: Config> =
        StorageValue<_, BoundedVec<QuarantinedPreimage<T>, T::MaxPendingPreimages>, ValueQuery>;
    #[pallet::storage]
    pub type PendingSystemOps<T: Config> =
        StorageValue<_, BoundedVec<PendingSystemOp<T>, T::MaxPendingSystemOps>, ValueQuery>;
    #[pallet::storage]
    pub type QuarantinedSystemOps<T: Config> =
        StorageValue<_, BoundedVec<QuarantinedSystemOp<T>, T::MaxPendingSystemOps>, ValueQuery>;
    #[pallet::storage]
    pub type PendingSystemOpKeys<T: Config> =
        StorageMap<_, Blake2_128Concat, Hash, (), OptionQuery>;
    #[pallet::storage]
    pub type SystemOpNonces<T: Config> = StorageMap<_, Blake2_128Concat, Hash, u64, ValueQuery>;
    #[pallet::storage]
    pub type PendingAllocations<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, PendingAllocation<T>, OptionQuery>;
    #[pallet::storage]
    pub type ProcessedAllocations<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, (), OptionQuery>;
    #[pallet::storage]
    pub type AllocationReceipts<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, AllocationReceipt<BalanceOf<T>>, OptionQuery>;
    #[pallet::storage]
    pub type PendingAllocationCount<T> = StorageValue<_, u32, ValueQuery>;

    #[pallet::genesis_config]
    #[derive(frame_support::DefaultNoBound)]
    pub struct GenesisConfig<T: Config> {
        pub protocol_state: Vec<(Vec<u8>, Vec<u8>)>,
        pub worker_account: Option<T::AccountId>,
        pub allocation_relayer: Option<T::AccountId>,
        #[serde(skip)]
        pub _phantom: core::marker::PhantomData<T>,
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            if let Some(account) = &self.worker_account {
                WorkerAccount::<T>::put(account);
            }
            if let Some(account) = &self.allocation_relayer {
                AllocationRelayer::<T>::put(account);
            }
            for (key, value) in &self.protocol_state {
                let key: [u8; 31] = key
                    .as_slice()
                    .try_into()
                    .expect("MiniJAM protocol state keys are 31 bytes");
                let value = StateValue::try_from(value.clone())
                    .expect("MiniJAM protocol state values fit StateValue");
                ProtocolState::<T>::insert(key, value);
            }
        }
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ReportSubmitted {
            package_hash: Hash,
        },
        ReportImported {
            package_hash: Hash,
            receipt_hash: Hash,
        },
        PackageFailed {
            package_hash: Hash,
            error_code: ExecutionErrorCode,
        },
        PreimageQueued {
            requester: u32,
            blob_hash: Hash,
            blob_len: u32,
        },
        PreimageFailed {
            requester: u32,
            blob_hash: Hash,
            blob_len: u32,
            error_code: ExecutionErrorCode,
        },
        SystemOpQueued {
            request_id: Hash,
            sender: Hash,
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
        AllocationQueued {
            allocation_id: u64,
            target_service: u32,
            amount: BalanceOf<T>,
        },
        AllocationProcessed {
            allocation_id: u64,
            target_service: u32,
            amount: BalanceOf<T>,
        },
        AllocationRelayerChanged {
            old: Option<T::AccountId>,
            new: T::AccountId,
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
    }

    #[pallet::error]
    pub enum Error<T> {
        WorkerNotConfigured,
        UnauthorizedWorker,
        InvalidReport,
        DuplicatePackage,
        TooManyPendingReports,
        InvalidPreimage,
        DuplicatePendingPreimage,
        TooManyPendingPreimages,
        InvalidSystemOp,
        DuplicatePendingSystemOp,
        TooManyPendingSystemOps,
        QuarantinedSystemOpNotFound,
        UnknownService,
        AllocationRelayerNotConfigured,
        UnauthorizedAllocation,
        ZeroAllocation,
        DuplicateAllocation,
        TooManyPendingAllocations,
        AllocationNotFound,
        AllocationAmountOverflow,
        AllocationTransitionNotConfirmed,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_finalize(block: BlockNumberFor<T>) {
            Self::execute_block_stf(block);
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::DbWeight::get().reads_writes(5, 5))]
        #[transactional]
        pub fn submit_report(
            origin: OriginFor<T>,
            canonical_report: CanonicalReportBytes,
        ) -> DispatchResult {
            Self::ensure_worker(origin)?;
            ensure!(!canonical_report.is_empty(), Error::<T>::InvalidReport);
            let projection = T::JamCoreExecutor::default()
                .project_report(&canonical_report)
                .map_err(|_| Error::<T>::InvalidReport)?;
            ensure!(
                !PackageStatuses::<T>::contains_key(projection.package_hash),
                Error::<T>::DuplicatePackage
            );
            PendingReports::<T>::try_mutate(|reports| {
                reports
                    .try_push(PendingReport {
                        package_hash: projection.package_hash,
                        canonical_report,
                    })
                    .map_err(|_| Error::<T>::TooManyPendingReports)
            })?;
            PackageStatuses::<T>::insert(projection.package_hash, PackageStatus::Pending);
            Self::deposit_event(Event::ReportSubmitted {
                package_hash: projection.package_hash,
            });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::DbWeight::get().writes(3))]
        pub fn pause_execution(origin: OriginFor<T>, paused: bool) -> DispatchResult {
            ensure_root(origin)?;
            ReportImportPaused::<T>::put(paused);
            PreimageImportPaused::<T>::put(paused);
            SystemOpsPaused::<T>::put(paused);
            Self::deposit_event(Event::ImportPaused { paused });
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::DbWeight::get().reads_writes(2, 2))]
        #[transactional]
        pub fn submit_preimage(
            origin: OriginFor<T>,
            canonical_preimage: CanonicalPreimageBytes,
        ) -> DispatchResult {
            let submitter = ensure_signed(origin)?;
            let state = FrameProtocolState::<T>(core::marker::PhantomData);
            let metadata = T::JamCoreExecutor::default()
                .validate_preimage_submission(&canonical_preimage, &state)
                .map_err(|_| Error::<T>::InvalidPreimage)?;
            let key = PreimageKeyV1::from(metadata);
            ensure!(
                !PendingPreimageKeys::<T>::contains_key(key),
                Error::<T>::DuplicatePendingPreimage
            );
            PendingPreimages::<T>::try_mutate(|pending| {
                pending
                    .try_push(PendingPreimage {
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

        #[pallet::call_index(3)]
        #[pallet::weight(T::DbWeight::get().reads_writes(3, 4))]
        #[transactional]
        pub fn submit_system_op(
            origin: OriginFor<T>,
            command: Box<SystemCommandV2>,
        ) -> DispatchResult {
            let submitter = ensure_signed(origin)?;
            Self::validate_system_command(&command)?;
            let sender = Self::system_op_sender(&submitter);
            let nonce = SystemOpNonces::<T>::get(sender);
            let op = SystemOpV2::new(sender, nonce, *command);
            ensure!(
                !PendingSystemOpKeys::<T>::contains_key(op.request_id),
                Error::<T>::DuplicatePendingSystemOp
            );
            PendingSystemOps::<T>::try_mutate(|pending| {
                pending
                    .try_push(PendingSystemOp {
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

        #[pallet::call_index(4)]
        #[pallet::weight(T::DbWeight::get().reads_writes(2, 2))]
        pub fn drop_quarantined_system_op(
            origin: OriginFor<T>,
            request_id: Hash,
        ) -> DispatchResult {
            ensure_root(origin)?;
            let mut removed = false;
            QuarantinedSystemOps::<T>::mutate(|ops| {
                if let Some(index) = ops.iter().position(|op| op.op.request_id == request_id) {
                    ops.swap_remove(index);
                    removed = true;
                }
            });
            ensure!(removed, Error::<T>::QuarantinedSystemOpNotFound);
            Self::deposit_event(Event::SystemOpDropped { request_id });
            Ok(())
        }

        #[pallet::call_index(5)]
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
                if let Some(index) = ops.iter().position(|op| op.op.request_id == request_id) {
                    retry = Some(ops.swap_remove(index));
                }
            });
            let retry = retry.ok_or(Error::<T>::QuarantinedSystemOpNotFound)?;
            PendingSystemOps::<T>::try_mutate(|pending| {
                pending
                    .try_push(PendingSystemOp {
                        submitter: retry.submitter,
                        op: retry.op,
                    })
                    .map_err(|_| Error::<T>::TooManyPendingSystemOps)
            })?;
            PendingSystemOpKeys::<T>::insert(request_id, ());
            Self::deposit_event(Event::SystemOpRetried { request_id });
            Ok(())
        }

        #[pallet::call_index(6)]
        #[pallet::weight(T::DbWeight::get().reads_writes(1, 1))]
        pub fn clear_quarantined_system_ops(origin: OriginFor<T>) -> DispatchResult {
            ensure_root(origin)?;
            let count = QuarantinedSystemOps::<T>::take().len() as u32;
            Self::deposit_event(Event::SystemOpQuarantineCleared { count });
            Ok(())
        }

        #[pallet::call_index(7)]
        #[pallet::weight(T::DbWeight::get().reads_writes(5, 5))]
        #[transactional]
        pub fn submit_allocation(
            origin: OriginFor<T>,
            allocation: AllocationV1<BalanceOf<T>>,
        ) -> DispatchResult {
            let submitter = Self::ensure_allocation_relayer(origin)?;
            ensure!(!allocation.amount.is_zero(), Error::<T>::ZeroAllocation);
            let jam_amount = allocation.amount.saturated_into::<u64>();
            ensure!(
                jam_amount.saturated_into::<BalanceOf<T>>() == allocation.amount,
                Error::<T>::AllocationAmountOverflow
            );
            ensure!(
                Self::service_exists(allocation.target_service),
                Error::<T>::UnknownService
            );
            ensure!(
                !ProcessedAllocations::<T>::contains_key(allocation.allocation_id)
                    && !PendingAllocations::<T>::contains_key(allocation.allocation_id),
                Error::<T>::DuplicateAllocation
            );
            ensure!(
                PendingAllocationCount::<T>::get() < T::MaxPendingAllocations::get(),
                Error::<T>::TooManyPendingAllocations
            );
            let allocation_id = allocation.allocation_id;
            let target_service = allocation.target_service;
            let amount = allocation.amount;
            PendingAllocations::<T>::insert(
                allocation_id,
                PendingAllocation {
                    submitter,
                    allocation: allocation.clone(),
                },
            );
            PendingAllocationCount::<T>::mutate(|count| *count = count.saturating_add(1));
            AllocationReceipts::<T>::insert(
                allocation_id,
                AllocationReceipt {
                    allocation,
                    status: AllocationStatus::Pending,
                },
            );
            Self::deposit_event(Event::AllocationQueued {
                allocation_id,
                target_service,
                amount,
            });
            Ok(())
        }

        #[pallet::call_index(8)]
        #[pallet::weight(T::DbWeight::get().reads_writes(2, 2))]
        pub fn set_allocation_relayer(
            origin: OriginFor<T>,
            new_relayer: T::AccountId,
        ) -> DispatchResult {
            ensure_root(origin)?;
            let old = AllocationRelayer::<T>::get();
            AllocationRelayer::<T>::put(&new_relayer);
            Self::deposit_event(Event::AllocationRelayerChanged {
                old,
                new: new_relayer,
            });
            Ok(())
        }
    }

    #[pallet::view_functions]
    impl<T: Config> Pallet<T> {
        pub fn get_package_status(package_hash: Hash) -> Option<PackageStatus> {
            PackageStatuses::<T>::get(package_hash)
        }
        pub fn get_package_failure(package_hash: Hash) -> Option<ExecutionErrorCode> {
            PackageFailures::<T>::get(package_hash)
        }
        pub fn get_execution_receipt_by_package_hash(package_hash: Hash) -> Option<Hash> {
            ExecutionReceiptsByPackageHash::<T>::get(package_hash)
        }
        pub fn get_last_execution_receipt() -> Option<Hash> {
            LastExecutionReceipt::<T>::get()
        }
        pub fn get_allocation(allocation_id: u64) -> Option<AllocationReceipt<BalanceOf<T>>> {
            AllocationReceipts::<T>::get(allocation_id)
        }
        pub fn is_allocation_processed(allocation_id: u64) -> bool {
            ProcessedAllocations::<T>::contains_key(allocation_id)
        }
        pub fn get_pending_allocations() -> Vec<PendingAllocation<T>> {
            let mut values: Vec<_> = PendingAllocations::<T>::iter_values().collect();
            values.sort_by_key(|item| item.allocation.allocation_id);
            values
        }
        pub fn get_pending_preimages() -> BoundedVec<PendingPreimage<T>, T::MaxPendingPreimages> {
            PendingPreimages::<T>::get()
        }
        pub fn get_quarantined_preimages(
        ) -> BoundedVec<QuarantinedPreimage<T>, T::MaxPendingPreimages> {
            QuarantinedPreimages::<T>::get()
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
        pub fn get_quarantined_system_ops(
        ) -> BoundedVec<QuarantinedSystemOp<T>, T::MaxPendingSystemOps> {
            QuarantinedSystemOps::<T>::get()
        }
        pub fn get_system_op(request_id: Hash) -> Option<SystemOpV2> {
            PendingSystemOps::<T>::get()
                .into_iter()
                .find(|pending| pending.op.request_id == request_id)
                .map(|pending| pending.op)
        }
        pub fn get_system_receipt(request_id: Hash) -> Option<StateValue> {
            ProtocolState::<T>::get(Self::system_receipt_state_key(&request_id))
        }
        pub fn get_system_op_nonce(sender: Hash) -> u64 {
            SystemOpNonces::<T>::get(sender)
        }
        pub fn get_system_service_info() -> Option<StateValue> {
            ProtocolState::<T>::get(Self::service_info_state_key(0))
        }
        pub fn get_service_info(service_id: u32) -> Option<StateValue> {
            ProtocolState::<T>::get(Self::service_info_state_key(service_id))
        }
        pub fn get_service_storage(service_id: u32, key: Vec<u8>) -> Option<StateValue> {
            let state_key = jp_core_primitives::state::StoreKey::new_service_storage_key(
                &service_id,
                &jp_core_primitives::simple::ByteSequence::from(key),
            )
            .to_state_key()
            .0;
            ProtocolState::<T>::get(state_key)
        }
        pub fn get_service_preimage(service_id: u32, code_hash: Hash) -> Option<StateValue> {
            let state_key = jp_core_primitives::state::StoreKey::new_preimage_key(
                &service_id,
                &jp_core_primitives::crypto::OpaqueHash(code_hash),
            )
            .to_state_key()
            .0;
            ProtocolState::<T>::get(state_key)
        }
        pub fn get_protocol_state(key: [u8; 31]) -> Option<StateValue> {
            ProtocolState::<T>::get(key)
        }
    }

    impl<T: Config> Pallet<T> {
        fn ensure_worker(origin: OriginFor<T>) -> Result<T::AccountId, DispatchError> {
            let who = ensure_signed(origin)?;
            let worker = WorkerAccount::<T>::get().ok_or(Error::<T>::WorkerNotConfigured)?;
            ensure!(who == worker, Error::<T>::UnauthorizedWorker);
            Ok(who)
        }
        fn ensure_allocation_relayer(origin: OriginFor<T>) -> Result<T::AccountId, DispatchError> {
            let who = ensure_signed(origin)?;
            let expected =
                AllocationRelayer::<T>::get().ok_or(Error::<T>::AllocationRelayerNotConfigured)?;
            ensure!(who == expected, Error::<T>::UnauthorizedAllocation);
            Ok(who)
        }
        pub fn pending_allocation_inputs() -> Vec<Vec<u8>> {
            Self::get_pending_allocations()
                .into_iter()
                .map(|pending| pending.allocation.encode())
                .collect()
        }
        pub fn consume_allocation(allocation_id: u64) -> DispatchResult {
            ensure!(
                !ProcessedAllocations::<T>::contains_key(allocation_id),
                Error::<T>::DuplicateAllocation
            );
            let receipt =
                ProtocolState::<T>::get(Self::allocation_receipt_state_key(allocation_id))
                    .ok_or(Error::<T>::AllocationTransitionNotConfirmed)?;
            ensure!(
                receipt.len() >= 5 && receipt[0] == 0,
                Error::<T>::AllocationTransitionNotConfirmed
            );
            Self::consume_allocation_after_transition(allocation_id)
                .map_err(|_| Error::<T>::AllocationNotFound.into())
        }
        fn consume_allocation_after_transition(allocation_id: u64) -> Result<(), ExecutionFailure> {
            if ProcessedAllocations::<T>::contains_key(allocation_id) {
                return Err(ExecutionFailure::Fatal);
            }
            let pending =
                PendingAllocations::<T>::take(allocation_id).ok_or(ExecutionFailure::Fatal)?;
            ProcessedAllocations::<T>::insert(allocation_id, ());
            PendingAllocationCount::<T>::mutate(|count| *count = count.saturating_sub(1));
            AllocationReceipts::<T>::insert(
                allocation_id,
                AllocationReceipt {
                    allocation: pending.allocation.clone(),
                    status: AllocationStatus::Processed,
                },
            );
            Self::deposit_event(Event::AllocationProcessed {
                allocation_id,
                target_service: pending.allocation.target_service,
                amount: pending.allocation.amount,
            });
            Ok(())
        }

        fn execute_block_stf(block: BlockNumberFor<T>) {
            let mut pending = PendingReports::<T>::take().into_inner();
            let max_reports = T::MaxExecutionReports::get() as usize;
            let reports: Vec<_> = if ReportImportPaused::<T>::get() {
                Vec::new()
            } else {
                pending.drain(..pending.len().min(max_reports)).collect()
            };
            let retained = pending;
            let result =
                with_transaction(|| match Self::execute_block_stf_inner(block, &reports) {
                    Ok(summary) => {
                        PendingReports::<T>::put(
                            BoundedVec::try_from(retained.clone())
                                .expect("pending report bound is preserved"),
                        );
                        TransactionOutcome::Commit(Ok(summary))
                    }
                    Err(error) => TransactionOutcome::Rollback(Err(error)),
                });
            match result {
                Ok(summary) => Self::deposit_event(Event::BlockStfExecuted {
                    slot: block.saturated_into(),
                    report_count: summary.report_count,
                    preimage_count: summary.preimage_count,
                    system_op_count: summary.system_op_count,
                    receipt_hash: summary.receipt_hash,
                }),
                Err(ExecutionFailure::ReportsFailed(code)) => {
                    for report in reports {
                        PackageStatuses::<T>::insert(report.package_hash, PackageStatus::Failed);
                        PackageFailures::<T>::insert(report.package_hash, code);
                        Self::deposit_event(Event::PackageFailed {
                            package_hash: report.package_hash,
                            error_code: code,
                        });
                    }
                    PendingReports::<T>::put(
                        BoundedVec::try_from(retained).expect("pending report bound is preserved"),
                    );
                }
                Err(ExecutionFailure::PreimagesRejected(code)) => {
                    Self::quarantine_pending_preimages(code)
                }
                Err(ExecutionFailure::SystemOpsYielded(outcome)) => {
                    Self::quarantine_pending_system_ops(outcome)
                }
                Err(ExecutionFailure::Fatal) => {
                    for report in reports {
                        PackageStatuses::<T>::insert(report.package_hash, PackageStatus::Failed);
                        PackageFailures::<T>::insert(
                            report.package_hash,
                            ExecutionErrorCode::InvalidOutput,
                        );
                        Self::deposit_event(Event::PackageFailed {
                            package_hash: report.package_hash,
                            error_code: ExecutionErrorCode::InvalidOutput,
                        });
                    }
                    PendingReports::<T>::put(
                        BoundedVec::try_from(retained).expect("pending report bound is preserved"),
                    );
                }
            }
        }

        fn execute_block_stf_inner(
            block: BlockNumberFor<T>,
            pending_reports: &[PendingReport],
        ) -> Result<BlockStfSummary, ExecutionFailure> {
            let reports: ReportBatch = pending_reports
                .iter()
                .map(|report| report.canonical_report.clone())
                .collect::<Vec<_>>()
                .try_into()
                .map_err(|_| ExecutionFailure::Fatal)?;
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
            let input = MiniJamExecutionInput {
                protocol_version: PROTOCOL_VERSION_V1,
                slot: block.saturated_into(),
                parent_hash: Self::host_parent_hash(block),
                parent_state_root: Self::host_parent_state_root(block),
                entropy: Self::host_entropy(block),
                reports,
                preimages,
                system_ops,
                max_gas: T::MaxExecutionGas::get(),
            };
            let state = FrameProtocolState::<T>(core::marker::PhantomData);
            let output = match T::JamCoreExecutor::default().execute(input.clone(), &state) {
                Ok(output) => output,
                Err(MiniJamError::Execution(outcome)) if !input.reports.is_empty() => {
                    return Err(ExecutionFailure::ReportsFailed(outcome.into()))
                }
                Err(MiniJamError::Execution(outcome)) if !input.system_ops.is_empty() => {
                    return Err(ExecutionFailure::SystemOpsYielded(outcome))
                }
                Err(MiniJamError::Execution(_)) => return Err(ExecutionFailure::Fatal),
                Err(MiniJamError::Input(_))
                    if !input.preimages.is_empty() && input.reports.is_empty() =>
                {
                    return Err(ExecutionFailure::PreimagesRejected(
                        ExecutionErrorCode::InvalidInput,
                    ))
                }
                Err(MiniJamError::Input(_)) if !input.reports.is_empty() => {
                    return Err(ExecutionFailure::ReportsFailed(
                        ExecutionErrorCode::InvalidInput,
                    ))
                }
                Err(MiniJamError::State(_) | MiniJamError::Invariant(_)) => {
                    return Err(ExecutionFailure::Fatal)
                }
                Err(MiniJamError::Input(_)) => return Err(ExecutionFailure::Fatal),
            };
            let delta = validate_execution_output(&input, &output, &state)
                .map_err(|error| Self::map_validation_error(&input, error))?;
            let changes = delta.changes().to_vec();
            Self::apply_delta(delta)?;
            Self::consume_successful_allocations(&changes)?;
            Self::consume_preimages(&output.consumed_preimages);
            Self::consume_system_ops(&output.consumed_system_ops);
            for report in pending_reports {
                if output.consumed_reports.contains(&report.package_hash)
                    || output
                        .consumed_reports
                        .contains(&blake2_256(&report.canonical_report))
                {
                    PackageStatuses::<T>::insert(report.package_hash, PackageStatus::Imported);
                    ExecutionReceiptsByPackageHash::<T>::insert(
                        report.package_hash,
                        output.receipt_hash,
                    );
                    Self::deposit_event(Event::ReportImported {
                        package_hash: report.package_hash,
                        receipt_hash: output.receipt_hash,
                    });
                }
            }
            LastExecutionReceipt::<T>::put(output.receipt_hash);
            Ok(BlockStfSummary {
                report_count: input.reports.len() as u32,
                preimage_count: input.preimages.len() as u32,
                system_op_count: input.system_ops.len() as u32,
                receipt_hash: output.receipt_hash,
            })
        }
        fn map_validation_error(
            input: &MiniJamExecutionInput,
            error: ValidationError,
        ) -> ExecutionFailure {
            let code = match error {
                ValidationError::GasExceeded => ExecutionErrorCode::GasExceeded,
                ValidationError::DeltaTooLarge => ExecutionErrorCode::DeltaTooLarge,
                ValidationError::State(_) | ValidationError::Invariant(_) => {
                    ExecutionErrorCode::InvalidOutput
                }
            };
            if !input.reports.is_empty() {
                ExecutionFailure::ReportsFailed(code)
            } else if !input.preimages.is_empty() {
                ExecutionFailure::PreimagesRejected(code)
            } else {
                ExecutionFailure::Fatal
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
                StateOperation::Upsert | StateOperation::Update => ProtocolState::<T>::insert(
                    change.key,
                    change.value.ok_or(ExecutionFailure::Fatal)?,
                ),
                StateOperation::Remove => ProtocolState::<T>::remove(change.key),
            }
            Ok(())
        }
        fn pending_preimage_batch() -> Result<PreimageBatch, ExecutionFailure> {
            let mut pending = PendingPreimages::<T>::get().into_inner();
            pending.sort_by_key(|item| {
                (
                    item.metadata.requester,
                    item.metadata.blob_hash,
                    item.metadata.blob_len,
                )
            });
            pending
                .into_iter()
                .map(|item| item.canonical)
                .collect::<Vec<_>>()
                .try_into()
                .map_err(|_| ExecutionFailure::Fatal)
        }
        fn pending_system_ops_batch() -> Result<SystemOpBatch, ExecutionFailure> {
            let mut pending = PendingSystemOps::<T>::get().into_inner();
            pending.sort_by_key(|item| (item.op.submitter, item.op.nonce, item.op.request_id));
            let mut ops: Vec<_> = pending.into_iter().map(|item| item.op).collect();
            for encoded in Self::pending_allocation_inputs() {
                let mut raw = encoded.as_slice();
                let allocation = AllocationV1::<BalanceOf<T>>::decode(&mut raw)
                    .map_err(|_| ExecutionFailure::Fatal)?;
                if !raw.is_empty() {
                    return Err(ExecutionFailure::Fatal);
                }
                ops.push(Self::allocation_system_op(&allocation)?);
            }
            ops.try_into().map_err(|_| ExecutionFailure::Fatal)
        }
        fn consume_successful_allocations(
            changes: &[ProtocolStateChange],
        ) -> Result<(), ExecutionFailure> {
            for pending in Self::get_pending_allocations() {
                let id = pending.allocation.allocation_id;
                if let Some(change) = changes
                    .iter()
                    .find(|change| change.key == Self::allocation_receipt_state_key(id))
                {
                    let value = change.value.as_ref().ok_or(ExecutionFailure::Fatal)?;
                    if value.first() == Some(&0) {
                        Self::consume_allocation_after_transition(id)?;
                    }
                }
            }
            Ok(())
        }
        fn consume_preimages(consumed: &[Hash]) {
            let consumed: BTreeSet<_> = consumed.iter().copied().collect();
            PendingPreimages::<T>::mutate(|pending| {
                let mut i = 0;
                while i < pending.len() {
                    if consumed.contains(&blake2_256(&pending[i].canonical)) {
                        PendingPreimageKeys::<T>::remove(PreimageKeyV1::from(pending[i].metadata));
                        pending.swap_remove(i);
                    } else {
                        i += 1;
                    }
                }
            });
        }
        fn consume_system_ops(consumed: &[Hash]) {
            let consumed: BTreeSet<_> = consumed.iter().copied().collect();
            PendingSystemOps::<T>::mutate(|pending| {
                let mut i = 0;
                while i < pending.len() {
                    if consumed.contains(&pending[i].op.request_id) {
                        let id = pending[i].op.request_id;
                        PendingSystemOpKeys::<T>::remove(id);
                        pending.swap_remove(i);
                        Self::deposit_event(Event::SystemOpConsumed { request_id: id });
                    } else {
                        i += 1;
                    }
                }
            });
        }
        fn quarantine_pending_preimages(error_code: ExecutionErrorCode) {
            let pending = PendingPreimages::<T>::take();
            let mut quarantined = QuarantinedPreimages::<T>::get();
            for item in pending {
                PendingPreimageKeys::<T>::remove(PreimageKeyV1::from(item.metadata));
                Self::deposit_event(Event::PreimageFailed {
                    requester: item.metadata.requester,
                    blob_hash: item.metadata.blob_hash,
                    blob_len: item.metadata.blob_len,
                    error_code,
                });
                let _ = quarantined.try_push(QuarantinedPreimage {
                    submitter: item.submitter,
                    metadata: item.metadata,
                    canonical_hash: blake2_256(&item.canonical),
                    error_code,
                    block_number: frame_system::Pallet::<T>::block_number(),
                    retryable: false,
                });
            }
            QuarantinedPreimages::<T>::put(quarantined);
        }
        fn quarantine_pending_system_ops(outcome: ExecutionOutcome) {
            let pending = PendingSystemOps::<T>::take();
            let mut quarantined = QuarantinedSystemOps::<T>::get();
            for item in pending {
                PendingSystemOpKeys::<T>::remove(item.op.request_id);
                Self::deposit_event(Event::SystemOpFailed {
                    request_id: item.op.request_id,
                    outcome: outcome.clone(),
                });
                let _ = quarantined.try_push(QuarantinedSystemOp {
                    submitter: item.submitter,
                    canonical_hash: blake2_256(&item.op.encode()),
                    op: item.op,
                    error_code: outcome.clone().into(),
                    block_number: frame_system::Pallet::<T>::block_number(),
                    retryable: true,
                });
            }
            QuarantinedSystemOps::<T>::put(quarantined);
        }
        fn validate_system_command(command: &SystemCommandV2) -> DispatchResult {
            match command {
                SystemCommandV2::CreateService {
                    code_len,
                    min_item_gas,
                    min_memo_gas,
                    ..
                } => ensure!(
                    *code_len > 0 && *min_item_gas > 0 && *min_memo_gas > 0,
                    Error::<T>::InvalidSystemOp
                ),
                SystemCommandV2::ApplyAllocation {
                    allocation_id,
                    target_service,
                    amount,
                } => ensure!(
                    *allocation_id > 0 && *target_service > 0 && *amount > 0,
                    Error::<T>::InvalidSystemOp
                ),
            }
            Ok(())
        }
        fn allocation_system_op(
            allocation: &AllocationV1<BalanceOf<T>>,
        ) -> Result<SystemOpV2, ExecutionFailure> {
            let amount = allocation.amount.saturated_into::<u64>();
            if amount.saturated_into::<BalanceOf<T>>() != allocation.amount
                || !Self::service_exists(allocation.target_service)
            {
                return Err(ExecutionFailure::Fatal);
            }
            Ok(SystemOpV2::new(
                ALLOCATION_SYSTEM_SENDER,
                allocation.allocation_id,
                SystemCommandV2::ApplyAllocation {
                    allocation_id: allocation.allocation_id,
                    target_service: allocation.target_service,
                    amount,
                },
            ))
        }
        fn allocation_receipt_state_key(allocation_id: u64) -> [u8; 31] {
            let mut key = ALLOCATION_RECEIPT_PREFIX.to_vec();
            key.extend_from_slice(&allocation_id.to_le_bytes());
            jp_core_primitives::state::StoreKey::new_service_storage_key(
                &SYSTEM_SERVICE_ID,
                &jp_core_primitives::simple::ByteSequence::from(key),
            )
            .to_state_key()
            .0
        }
        fn system_op_sender(account: &T::AccountId) -> Hash {
            blake2_256(&account.encode())
        }
        fn service_exists(service_id: u32) -> bool {
            ProtocolState::<T>::contains_key(Self::service_info_state_key(service_id))
        }
        fn system_receipt_state_key(request_id: &Hash) -> [u8; 31] {
            let mut key = SYSTEM_STORAGE_RECEIPT_PREFIX.to_vec();
            key.extend_from_slice(request_id);
            jp_core_primitives::state::StoreKey::new_service_storage_key(
                &SYSTEM_SERVICE_ID,
                &jp_core_primitives::simple::ByteSequence::from(key),
            )
            .to_state_key()
            .0
        }
        fn service_info_state_key(service_id: u32) -> [u8; 31] {
            let service = service_id.to_le_bytes();
            let mut key = [0; 31];
            key[0] = 0xff;
            key[1] = service[0];
            key[3] = service[1];
            key[5] = service[2];
            key[7] = service[3];
            key
        }
        fn host_parent_hash(block: BlockNumberFor<T>) -> Hash {
            let parent = frame_system::Pallet::<T>::block_hash(block.saturating_sub(One::one()));
            blake2_256(&parent.encode())
        }
        fn host_parent_state_root(block: BlockNumberFor<T>) -> Hash {
            blake2_256(&(b"minijam/parent-state-root", Self::host_parent_hash(block)).encode())
        }
        fn host_entropy(block: BlockNumberFor<T>) -> Hash {
            blake2_256(&(b"minijam/host-entropy", block).encode())
        }
    }

    pub struct FrameProtocolState<T: Config>(core::marker::PhantomData<T>);
    impl<T: Config> ProtocolStateReader for FrameProtocolState<T> {
        fn get(&self, key: &[u8; 31]) -> Result<Option<Vec<u8>>, StateError> {
            Ok(ProtocolState::<T>::get(key).map(|value| value.into_inner()))
        }
    }
    enum ExecutionFailure {
        ReportsFailed(ExecutionErrorCode),
        SystemOpsYielded(ExecutionOutcome),
        PreimagesRejected(ExecutionErrorCode),
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
