// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "4096"]

extern crate alloc;

use alloc::vec::Vec;

#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

pub mod apis;
mod genesis_config_presets;

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
    pub BridgeEscrowAccount: AccountId = AccountId::new([8; 32]);
    pub const MinimumWorkerStake: Balance = 1_000 * UNIT;
    pub const TimelyVoteReward: Balance = UNIT;
    pub const MinimumAbsenceSlash: Balance = UNIT;
    pub const AbsenceSlash: Perbill = Perbill::from_percent(1);
    pub const EquivocationSlash: Perbill = Perbill::from_percent(20);
    pub const WorkDeposit: Balance = 10 * UNIT;
    pub const CandidateBond: Balance = 10 * UNIT;
    pub const CandidateRejectionSlash: Balance = UNIT;
    pub const AcceptedSubmitterReward: Balance = UNIT;
    pub const RefineGasPrice: Balance = 1;
    pub const AccumulateGasPrice: Balance = 1;
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
    type WorkersPerWork = ConstU32<3>;
    type MaxWorksPerRound = ConstU32<4>;
    type MaxDutiesPerWorkerPerRound = ConstU32<2>;
    type SupportThreshold = ConstU32<2>;
    type OpposeThreshold = ConstU32<2>;
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
    type RefineGasPrice = RefineGasPrice;
    type AccumulateGasPrice = AccumulateGasPrice;
    type ReportSubmissionDeadline = ConstU32<20>;
    type VoteWindow = ConstU32<10>;
    type MaxCandidateRounds = ConstU8<3>;
    type MaxPendingWorks = ConstU32<64>;
    type MaxExecutionReports = ConstU32<4>;
    type MaxExecutionGas = ConstU64<10_000_000>;
    type MaxWorkPackageBytes = ConstU32<1_048_576>;
    type JamCoreExecutor = jambda_minijam_executive::MiniJamExecutive;
    type MaxPendingPreimages = ConstU32<64>;
    type MaxPendingSystemOps = ConstU32<64>;
}

impl pallet_minijam_bridge::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type RuntimeHoldReason = RuntimeHoldReason;
    type EscrowAccount = BridgeEscrowAccount;
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
    #[runtime::pallet_index(9)]
    pub type MiniJamBridge = pallet_minijam_bridge;
}
