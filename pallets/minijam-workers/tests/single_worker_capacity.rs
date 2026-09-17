use frame_support::{derive_impl, parameter_types};
use sp_runtime::BuildStorage;

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
    pub struct Test;

    #[runtime::pallet_index(0)]
    pub type System = frame_system::Pallet<Test>;

    #[runtime::pallet_index(1)]
    pub type Balances = pallet_balances::Pallet<Test>;

    #[runtime::pallet_index(2)]
    pub type Workers = pallet_minijam_workers::Pallet<Test>;
}

type Block = frame_system::mocking::MockBlock<Test>;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type AccountData = pallet_balances::AccountData<u128>;
}

parameter_types! {
    pub const ExistentialDeposit: u128 = 1;
    pub const MinimumStake: u128 = 1_000;
    pub const ChainId: [u8; 32] = [42; 32];
    pub const RewardPool: u64 = 100;
    pub const TimelyVoteReward: u128 = 10;
    pub const MinimumAbsenceSlash: u128 = 10;
    pub const AbsenceSlash: sp_runtime::Perbill = sp_runtime::Perbill::from_percent(1);
    pub const EquivocationSlash: sp_runtime::Perbill = sp_runtime::Perbill::from_percent(20);
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
    type AccountStore = System;
    type Balance = u128;
    type ExistentialDeposit = ExistentialDeposit;
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
}

impl pallet_minijam_workers::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type RuntimeHoldReason = RuntimeHoldReason;
    type MinimumStake = MinimumStake;
    type EpochLength = frame_support::traits::ConstU32<10>;
    type MaxCandidates = frame_support::traits::ConstU32<8>;
    type TopWorkers = frame_support::traits::ConstU32<1>;
    type AssignmentSeedDelay = frame_support::traits::ConstU32<10>;
    type WorkersPerWork = frame_support::traits::ConstU32<1>;
    type MaxWorksPerRound = frame_support::traits::ConstU32<64>;
    type MaxDutiesPerWorkerPerRound = frame_support::traits::ConstU32<64>;
    type SupportThreshold = frame_support::traits::ConstU32<1>;
    type OpposeThreshold = frame_support::traits::ConstU32<1>;
    type MaxOpenVotes = frame_support::traits::ConstU32<4>;
    type ChainId = ChainId;
    type ProtocolVersion =
        frame_support::traits::ConstU16<{ minijam_protocol::PROTOCOL_VERSION_V1 }>;
    type RewardPool = RewardPool;
    type TimelyVoteReward = TimelyVoteReward;
    type AbsenceSlash = AbsenceSlash;
    type MinimumAbsenceSlash = MinimumAbsenceSlash;
    type EquivocationSlash = EquivocationSlash;
    type EquivocationSuspension = frame_support::traits::ConstU32<2>;
}

fn test_ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(1, 100_001), (100, 1_000_001)],
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .unwrap();
    pallet_minijam_workers::GenesisConfig::<Test> {
        workers: vec![(1, [1; 32], 1_000)],
        _phantom: Default::default(),
    }
    .assimilate_storage(&mut storage)
    .unwrap();
    storage.into()
}

#[test]
fn single_worker_sustains_sixteen_assignments_in_one_epoch_and_round() {
    test_ext().execute_with(|| {
        for work_id in 0..16 {
            let assigned = Workers::assign_work(work_id, 0).unwrap();
            assert_eq!(assigned.as_slice(), &[0]);
        }

        assert_eq!(
            pallet_minijam_workers::DutyCounts::<Test>::get((0, 0, 0)),
            16
        );
        assert_eq!(
            pallet_minijam_workers::AssignedWorkCount::<Test>::get((0, 0)),
            16
        );
    });
}
