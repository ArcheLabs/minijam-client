use crate as pallet_minijam_workers;
use frame_support::{
    assert_noop, assert_ok, derive_impl, parameter_types, traits::tokens::fungible::InspectHold,
};
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

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

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type AccountData = pallet_balances::AccountData<u128>;
}

parameter_types! {
    pub const ExistentialDeposit: u128 = 1;
    pub const MinimumStake: u128 = 1_000;
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
    type TopWorkers = frame_support::traits::ConstU32<3>;
}

fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(1, 10_001), (2, 10_001), (3, 10_001), (4, 10_001)],
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .unwrap();
    storage.into()
}

#[test]
fn registration_holds_stake_and_is_delayed_until_next_epoch() {
    new_test_ext().execute_with(|| {
        assert_ok!(Workers::register(RuntimeOrigin::signed(1), [1; 32], 2_000));
        let reason: RuntimeHoldReason = pallet_minijam_workers::HoldReason::WorkerStake.into();
        assert_eq!(Balances::balance_on_hold(&reason, &1), 2_000);
        assert!(Workers::active_workers().is_empty());

        System::set_block_number(10);
        <Workers as frame_support::traits::Hooks<u64>>::on_initialize(10);
        assert_eq!(Workers::current_epoch(), 1);
        assert_eq!(Workers::active_workers().as_slice(), &[0]);
    });
}

#[test]
fn snapshot_orders_by_stake_then_worker_id() {
    new_test_ext().execute_with(|| {
        assert_ok!(Workers::register(RuntimeOrigin::signed(1), [1; 32], 2_000));
        assert_ok!(Workers::register(RuntimeOrigin::signed(2), [2; 32], 3_000));
        assert_ok!(Workers::register(RuntimeOrigin::signed(3), [3; 32], 3_000));
        assert_ok!(Workers::register(RuntimeOrigin::signed(4), [4; 32], 4_000));

        System::set_block_number(10);
        <Workers as frame_support::traits::Hooks<u64>>::on_initialize(10);
        assert_eq!(Workers::active_workers().as_slice(), &[3, 1, 2]);
    });
}

#[test]
fn registration_enforces_minimum_and_uniqueness() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Workers::register(RuntimeOrigin::signed(1), [1; 32], 999),
            pallet_minijam_workers::Error::<Test>::StakeBelowMinimum
        );
        assert_ok!(Workers::register(RuntimeOrigin::signed(1), [1; 32], 1_000));
        assert_noop!(
            Workers::register(RuntimeOrigin::signed(1), [2; 32], 1_000),
            pallet_minijam_workers::Error::<Test>::AlreadyRegistered
        );
    });
}
