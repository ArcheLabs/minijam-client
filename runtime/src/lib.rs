// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "4096"]

extern crate alloc;

use alloc::vec::Vec;

#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

pub mod apis;
pub mod genesis_config_presets;

use frame_support::{
    derive_impl, parameter_types,
    traits::{ConstBool, ConstU128, ConstU32, ConstU64, ConstU8, VariantCountOf},
    weights::{
        constants::{RocksDbWeight, WEIGHT_REF_TIME_PER_SECOND},
        IdentityFee, Weight,
    },
};
use frame_system::limits::{BlockLength, BlockWeights};
use pallet_transaction_payment::{ConstFeeMultiplier, FungibleAdapter, Multiplier};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_runtime::{
    generic, impl_opaque_keys,
    traits::{BlakeTwo256, IdentifyAccount, One, Verify},
    MultiAddress, MultiSignature, Perbill,
};
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

pub const MILLI_SECS_PER_BLOCK: u64 = 6_000;
pub const SLOT_DURATION: u64 = MILLI_SECS_PER_BLOCK;
pub const BLOCK_HASH_COUNT: BlockNumber = 2_400;
pub const UNIT: Balance = 1_000_000_000_000;
pub const EXISTENTIAL_DEPOSIT: Balance = 1_000_000_000;

pub type Signature = MultiSignature;
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;
pub type Balance = u128;
pub type Nonce = u32;
pub type Hash = sp_core::H256;
pub type BlockNumber = u32;
pub type Address = MultiAddress<AccountId, ()>;
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
pub type SignedBlock = generic::SignedBlock<Block>;
pub type BlockId = generic::BlockId<Block>;

pub mod opaque {
    use super::*;
    use sp_runtime::traits::{BlakeTwo256, Hash as HashT};

    pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;
    pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
    pub type Block = generic::Block<Header, UncheckedExtrinsic>;
    pub type BlockId = generic::BlockId<Block>;
    pub type Hash = <BlakeTwo256 as HashT>::Output;
}

impl_opaque_keys! {
    pub struct SessionKeys {
        pub aura: Aura,
        pub grandpa: Grandpa,
    }
}

#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
    spec_name: alloc::borrow::Cow::Borrowed("minijam"),
    impl_name: alloc::borrow::Cow::Borrowed("minijam"),
    authoring_version: 1,
    spec_version: 1,
    impl_version: 1,
    apis: apis::RUNTIME_API_VERSIONS,
    transaction_version: 1,
    system_version: 1,
};

#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
    NativeVersion {
        runtime_version: VERSION,
        can_author_with: Default::default(),
    }
}

pub type TxExtension = (
    frame_system::AuthorizeCall<Runtime>,
    frame_system::CheckNonZeroSender<Runtime>,
    frame_system::CheckSpecVersion<Runtime>,
    frame_system::CheckTxVersion<Runtime>,
    frame_system::CheckGenesis<Runtime>,
    frame_system::CheckEra<Runtime>,
    frame_system::CheckNonce<Runtime>,
    frame_system::CheckWeight<Runtime>,
    pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
    frame_system::WeightReclaim<Runtime>,
);
pub type UncheckedExtrinsic =
    generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, TxExtension>;
pub type SignedPayload = generic::SignedPayload<RuntimeCall, TxExtension>;
pub type Executive = frame_executive::Executive<
    Runtime,
    Block,
    frame_system::ChainContext<Runtime>,
    Runtime,
    AllPalletsWithSystem,
>;

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

parameter_types! {
    pub const BlockHashCount: BlockNumber = BLOCK_HASH_COUNT;
    pub const Version: RuntimeVersion = VERSION;
    pub RuntimeBlockWeights: BlockWeights = BlockWeights::with_sensible_defaults(
        Weight::from_parts(2 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
        NORMAL_DISPATCH_RATIO,
    );
    pub RuntimeBlockLength: BlockLength = BlockLength::builder()
        .max_length(5 * 1024 * 1024)
        .build();
    pub const SS58Prefix: u8 = 42;
    pub FeeMultiplier: Multiplier = Multiplier::one();
    pub const MiniJamChainId: [u8; 32] = [77; 32];
    pub RewardPoolAccount: AccountId = AccountId::new([9; 32]);
    pub FuelEscrowAccount: AccountId = AccountId::new([7; 32]);
    pub FaucetAccount: AccountId =
        AccountId::new(genesis_config_presets::STAGE0_FAUCET_ACCOUNT);
    pub const MinimumWorkerStake: Balance = 1_000 * UNIT;
    pub const TimelyVoteReward: Balance = 0;
    pub const MinimumAbsenceSlash: Balance = UNIT;
    pub const AbsenceSlash: Perbill = Perbill::from_percent(1);
    pub const EquivocationSlash: Perbill = Perbill::from_percent(20);
    pub const WorkDeposit: Balance = 0;
    pub const CandidateBond: Balance = 0;
    pub const CandidateRejectionSlash: Balance = 0;
    pub const AcceptedSubmitterReward: Balance = 0;
    pub const FaucetDripAmount: Balance = 100 * UNIT;
    pub const FaucetCooldownBlocks: BlockNumber = 100;
    pub const RefineGasPrice: Balance = 0;
    pub const AccumulateGasPrice: Balance = 0;
}

#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
impl frame_system::Config for Runtime {
    type Block = Block;
    type BlockWeights = RuntimeBlockWeights;
    type BlockLength = RuntimeBlockLength;
    type AccountId = AccountId;
    type Nonce = Nonce;
    type Hash = Hash;
    type BlockHashCount = BlockHashCount;
    type DbWeight = RocksDbWeight;
    type Version = Version;
    type AccountData = pallet_balances::AccountData<Balance>;
    type SS58Prefix = SS58Prefix;
    type MaxConsumers = ConstU32<16>;
}

impl pallet_timestamp::Config for Runtime {
    type Moment = u64;
    type OnTimestampSet = Aura;
    type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
    type WeightInfo = ();
}

impl pallet_aura::Config for Runtime {
    type AuthorityId = AuraId;
    type DisabledValidators = ();
    type MaxAuthorities = ConstU32<32>;
    type AllowMultipleBlocksPerSlot = ConstBool<false>;
    type SlotDuration = pallet_aura::MinimumPeriodTimesTwo<Runtime>;
}

impl pallet_grandpa::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    type MaxAuthorities = ConstU32<32>;
    type MaxNominators = ConstU32<0>;
    type MaxSetIdSessionEntries = ConstU64<0>;
    type KeyOwnerProof = sp_core::Void;
    type EquivocationReportSystem = ();
}

impl pallet_balances::Config for Runtime {
    type MaxLocks = ConstU32<50>;
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type Balance = Balance;
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = ();
    type ExistentialDeposit = ConstU128<EXISTENTIAL_DEPOSIT>;
    type AccountStore = System;
    type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
    type FreezeIdentifier = RuntimeFreezeReason;
    type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type DoneSlashHandler = ();
}

impl pallet_transaction_payment::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction = FungibleAdapter<Balances, ()>;
    type OperationalFeeMultiplier = ConstU8<5>;
    type WeightToFee = IdentityFee<Balance>;
    type LengthToFee = IdentityFee<Balance>;
    type FeeMultiplierUpdate = ConstFeeMultiplier<FeeMultiplier>;
    type WeightInfo = pallet_transaction_payment::weights::SubstrateWeight<Runtime>;
}

impl pallet_sudo::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type WeightInfo = pallet_sudo::weights::SubstrateWeight<Runtime>;
}

impl pallet_minijam_workers::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type RuntimeHoldReason = RuntimeHoldReason;
    type MinimumStake = MinimumWorkerStake;
    type EpochLength = ConstU32<100>;
    type MaxCandidates = ConstU32<256>;
    type TopWorkers = ConstU32<8>;
    type AssignmentSeedDelay = ConstU32<10>;
    type WorkersPerWork = ConstU32<1>;
    type MaxWorksPerRound = ConstU32<4>;
    type MaxDutiesPerWorkerPerRound = ConstU32<2>;
    type SupportThreshold = ConstU32<1>;
    type OpposeThreshold = ConstU32<1>;
    type MaxOpenVotes = ConstU32<64>;
    type ChainId = MiniJamChainId;
    type ProtocolVersion = frame_support::traits::ConstU16<1>;
    type RewardPool = RewardPoolAccount;
    type TimelyVoteReward = TimelyVoteReward;
    type AbsenceSlash = AbsenceSlash;
    type MinimumAbsenceSlash = MinimumAbsenceSlash;
    type EquivocationSlash = EquivocationSlash;
    type EquivocationSuspension = ConstU32<2>;
}

impl pallet_minijam::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type JamHoldReason = RuntimeHoldReason;
    type ChainId = MiniJamChainId;
    type WorkDeposit = WorkDeposit;
    type CandidateBond = CandidateBond;
    type CandidateRejectionSlash = CandidateRejectionSlash;
    type AcceptedSubmitterReward = AcceptedSubmitterReward;
    type RewardPool = RewardPoolAccount;
    type FuelEscrowAccount = FuelEscrowAccount;
    type FaucetAccount = FaucetAccount;
    type FaucetDripAmount = FaucetDripAmount;
    type FaucetCooldownBlocks = FaucetCooldownBlocks;
    type RefineGasPrice = RefineGasPrice;
    type AccumulateGasPrice = AccumulateGasPrice;
    type ReportSubmissionDeadline = ConstU32<20>;
    type VoteWindow = ConstU32<10>;
    type MaxCandidateRounds = ConstU8<3>;
    type MaxPendingWorks = ConstU32<64>;
    type MaxExecutionReports = ConstU32<4>;
    type MaxExecutionGas = ConstU64<20_000_000>;
    type MaxWorkPackageBytes = ConstU32<1_048_576>;
    type MaxBundleBytes = ConstU64<16_777_216>;
    type MaxServicesPerWork = ConstU32<64>;
    type JamCoreExecutor = jambda_minijam_executive::MiniJamExecutive;
    type MaxPendingPreimages = ConstU32<64>;
    type MaxPendingSystemOps = ConstU32<64>;
    type MaxPendingAllocations = ConstU32<64>;
}

#[frame_support::runtime]
mod runtime {
    #[runtime::runtime]
    #[runtime::derive(
        RuntimeCall,
        RuntimeEvent,
        RuntimeError,
        RuntimeOrigin,
        RuntimeFreezeReason,
        RuntimeHoldReason,
        RuntimeSlashReason,
        RuntimeLockId,
        RuntimeTask,
        RuntimeViewFunction
    )]
    pub struct Runtime;

    #[runtime::pallet_index(0)]
    pub type System = frame_system;
    #[runtime::pallet_index(1)]
    pub type Timestamp = pallet_timestamp;
    #[runtime::pallet_index(2)]
    pub type Aura = pallet_aura;
    #[runtime::pallet_index(3)]
    pub type Grandpa = pallet_grandpa;
    #[runtime::pallet_index(4)]
    pub type Balances = pallet_balances;
    #[runtime::pallet_index(5)]
    pub type TransactionPayment = pallet_transaction_payment;
    #[runtime::pallet_index(6)]
    pub type Sudo = pallet_sudo;
    #[runtime::pallet_index(7)]
    pub type MiniJamWorkers = pallet_minijam_workers;
    #[runtime::pallet_index(8)]
    pub type MiniJam = pallet_minijam;
}

#[cfg(test)]
mod stage0_economics_tests {
    use super::*;
    use frame_support::{
        assert_noop, assert_ok,
        traits::{Get, Hooks},
    };
    use minijam_protocol::SystemCommandV1;
    use sp_runtime::BuildStorage;

    fn runtime_ext() -> sp_io::TestExternalities {
        let storage = frame_system::GenesisConfig::<Runtime>::default()
            .build_storage()
            .expect("runtime system genesis builds");
        let mut ext: sp_io::TestExternalities = storage.into();
        ext.execute_with(|| {
            pallet_minijam::IngressRelayer::<Runtime>::put(AccountId::new(
                crate::genesis_config_presets::LOCAL_PLAYGROUND_RELAYER_ACCOUNT,
            ));
        });
        ext
    }

    #[test]
    #[ignore = "long-running Jambda cross-epoch integration; executed by the release gate"]
    fn real_pallet_executes_create_after_epoch_transitions() {
        runtime_ext().execute_with(|| {
            for (key, value) in crate::genesis_config_presets::system_service_zero_protocol_state()
            {
                pallet_minijam::ProtocolState::<Runtime>::insert(
                    <[u8; 31]>::try_from(key).unwrap(),
                    minijam_protocol::StateValue::try_from(value).unwrap(),
                );
            }
            let controller = [0x5a; 32];
            assert_ok!(MiniJam::submit_system_op(
                RuntimeOrigin::signed(AccountId::new(
                    crate::genesis_config_presets::LOCAL_PLAYGROUND_RELAYER_ACCOUNT,
                )),
                Box::new(SystemCommandV1::CreateService {
                    controller,
                    code_hash: [0x9b; 32],
                    code_len: 27,
                    min_item_gas: 2,
                    min_memo_gas: 3,
                }),
            ));
            System::set_block_number(122);
            MiniJam::on_finalize(122);
        });
    }

    #[test]
    fn user_economic_charges_are_zero() {
        assert_eq!(WorkDeposit::get(), 0);
        assert_eq!(CandidateBond::get(), 0);
        assert_eq!(CandidateRejectionSlash::get(), 0);
        assert_eq!(AcceptedSubmitterReward::get(), 0);
        assert_eq!(TimelyVoteReward::get(), 0);
        assert_eq!(RefineGasPrice::get(), 0);
        assert_eq!(AccumulateGasPrice::get(), 0);
    }

    #[test]
    fn stage0_execution_gas_covers_refine_and_accumulate_limits() {
        assert!(
            <<Runtime as pallet_minijam::Config>::MaxExecutionGas as Get<u64>>::get()
                >= minijam_protocol::stage0::REFINE_GAS_LIMIT
                    .saturating_add(minijam_protocol::stage0::ACCUMULATE_GAS_LIMIT)
        );
    }

    #[test]
    fn runtime_ext_configures_only_local_playground_relayer() {
        runtime_ext().execute_with(|| {
            let relayer =
                AccountId::new(crate::genesis_config_presets::LOCAL_PLAYGROUND_RELAYER_ACCOUNT);
            let direct_user = AccountId::new([0x93; 32]);

            assert_eq!(
                pallet_minijam::IngressRelayer::<Runtime>::get(),
                Some(relayer)
            );
            assert_ne!(
                pallet_minijam::IngressRelayer::<Runtime>::get(),
                Some(direct_user)
            );
        });
    }

    #[test]
    fn runtime_dispatch_enforces_system_and_preimage_ingress() {
        runtime_ext().execute_with(|| {
            let relayer =
                AccountId::new(crate::genesis_config_presets::LOCAL_PLAYGROUND_RELAYER_ACCOUNT);
            let direct_user = AccountId::new([0x93; 32]);
            let command = SystemCommandV1::CreateService {
                controller: [0x44; 32],
                code_hash: [0x55; 32],
                code_len: 32,
                min_item_gas: 1,
                min_memo_gas: 1,
            };

            assert_noop!(
                MiniJam::submit_system_op(
                    RuntimeOrigin::signed(direct_user.clone()),
                    Box::new(command.clone())
                ),
                pallet_minijam::Error::<Runtime>::UnauthorizedIngress
            );
            assert_ok!(MiniJam::submit_system_op(
                RuntimeOrigin::signed(relayer.clone()),
                Box::new(command)
            ));

            let malformed_preimage: minijam_protocol::CanonicalPreimageBytes =
                vec![0xff].try_into().unwrap();
            assert_noop!(
                MiniJam::submit_preimage(
                    RuntimeOrigin::signed(direct_user),
                    malformed_preimage.clone()
                ),
                pallet_minijam::Error::<Runtime>::UnauthorizedIngress
            );
            assert_noop!(
                MiniJam::submit_preimage(RuntimeOrigin::signed(relayer), malformed_preimage),
                pallet_minijam::Error::<Runtime>::InvalidPreimage
            );
        });
    }
}
