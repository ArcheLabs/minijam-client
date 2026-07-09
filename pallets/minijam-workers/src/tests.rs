use crate as pallet_minijam_workers;
use frame_support::{
    assert_noop, assert_ok, derive_impl, parameter_types,
    traits::tokens::fungible::{Inspect, InspectHold},
};
use minijam_protocol::{Verdict, WorkerVoteV1, PROTOCOL_VERSION_V1};
use sp_core::{sr25519, Pair};
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
    pub const ChainId: [u8; 32] = [42; 32];
    pub const RewardPool: u64 = 100;
    pub const TimelyVoteReward: u128 = 10;
    pub const MinimumAbsenceSlash: u128 = 10;
    pub const AbsenceSlash: sp_runtime::Perbill = sp_runtime::Perbill::from_percent(1);
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
    type AssignmentSeedDelay = frame_support::traits::ConstU32<10>;
    type WorkersPerWork = frame_support::traits::ConstU32<3>;
    type MaxWorksPerRound = frame_support::traits::ConstU32<4>;
    type MaxDutiesPerWorkerPerRound = frame_support::traits::ConstU32<2>;
    type SupportThreshold = frame_support::traits::ConstU32<2>;
    type OpposeThreshold = frame_support::traits::ConstU32<2>;
    type MaxOpenVotes = frame_support::traits::ConstU32<4>;
    type ChainId = ChainId;
    type ProtocolVersion = frame_support::traits::ConstU16<PROTOCOL_VERSION_V1>;
    type RewardPool = RewardPool;
    type TimelyVoteReward = TimelyVoteReward;
    type AbsenceSlash = AbsenceSlash;
    type MinimumAbsenceSlash = MinimumAbsenceSlash;
}

fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (1, 10_001),
            (2, 10_001),
            (3, 10_001),
            (4, 10_001),
            (100, 1_000_001),
        ],
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

#[test]
fn stake_and_key_updates_activate_on_next_epoch() {
    new_test_ext().execute_with(|| {
        assert_ok!(Workers::register(RuntimeOrigin::signed(1), [1; 32], 2_000));
        System::set_block_number(10);
        <Workers as frame_support::traits::Hooks<u64>>::on_initialize(10);

        assert_ok!(Workers::schedule_update(
            RuntimeOrigin::signed(1),
            Some([9; 32]),
            Some(4_000)
        ));
        let reason: RuntimeHoldReason = pallet_minijam_workers::HoldReason::WorkerStake.into();
        assert_eq!(Balances::balance_on_hold(&reason, &1), 4_000);
        assert_eq!(
            pallet_minijam_workers::Workers::<Test>::get(0)
                .unwrap()
                .active_stake,
            2_000
        );

        System::set_block_number(20);
        <Workers as frame_support::traits::Hooks<u64>>::on_initialize(20);
        let worker = pallet_minijam_workers::Workers::<Test>::get(0).unwrap();
        assert_eq!(worker.active_stake, 4_000);
        assert_eq!(worker.session_key, [9; 32]);
    });
}

#[test]
fn reduced_stake_is_released_after_two_epochs() {
    new_test_ext().execute_with(|| {
        assert_ok!(Workers::register(RuntimeOrigin::signed(1), [1; 32], 4_000));
        System::set_block_number(10);
        <Workers as frame_support::traits::Hooks<u64>>::on_initialize(10);
        assert_ok!(Workers::schedule_update(
            RuntimeOrigin::signed(1),
            None,
            Some(2_000)
        ));

        let reason: RuntimeHoldReason = pallet_minijam_workers::HoldReason::WorkerStake.into();
        System::set_block_number(20);
        <Workers as frame_support::traits::Hooks<u64>>::on_initialize(20);
        assert_eq!(Balances::balance_on_hold(&reason, &1), 4_000);

        System::set_block_number(30);
        <Workers as frame_support::traits::Hooks<u64>>::on_initialize(30);
        assert_eq!(Balances::balance_on_hold(&reason, &1), 4_000);

        System::set_block_number(40);
        <Workers as frame_support::traits::Hooks<u64>>::on_initialize(40);
        assert_eq!(Balances::balance_on_hold(&reason, &1), 2_000);
        assert!(!pallet_minijam_workers::Unbonding::<Test>::contains_key(0));
    });
}

#[test]
fn assignment_is_distinct_idempotent_and_bounded() {
    new_test_ext().execute_with(|| {
        for account in 1..=4 {
            assert_ok!(Workers::register(
                RuntimeOrigin::signed(account),
                [account as u8; 32],
                1_000 + u128::from(account)
            ));
        }
        System::set_block_number(10);
        <Workers as frame_support::traits::Hooks<u64>>::on_initialize(10);

        let first = Workers::assign_work(7, 0).unwrap();
        assert_eq!(first.len(), 3);
        assert_ne!(first[0], first[1]);
        assert_ne!(first[1], first[2]);
        assert_eq!(Workers::assign_work(7, 0).unwrap(), first);
    });
}

#[test]
fn assignment_refuses_to_lower_k() {
    new_test_ext().execute_with(|| {
        for account in 1..=2 {
            assert_ok!(Workers::register(
                RuntimeOrigin::signed(account),
                [account as u8; 32],
                1_000
            ));
        }
        System::set_block_number(10);
        <Workers as frame_support::traits::Hooks<u64>>::on_initialize(10);
        assert_eq!(
            Workers::assign_work(8, 0),
            Err(pallet_minijam_workers::Error::<Test>::InsufficientWorkers.into())
        );
    });
}

fn setup_signed_voting() -> (Vec<sr25519::Pair>, Vec<u64>) {
    let pairs: Vec<_> = (1u8..=3)
        .map(|seed| sr25519::Pair::from_seed(&[seed; 32]))
        .collect();
    for (index, pair) in pairs.iter().enumerate() {
        assert_ok!(Workers::register(
            RuntimeOrigin::signed((index + 1) as u64),
            pair.public().0,
            1_000 + index as u128
        ));
    }
    System::set_block_number(10);
    <Workers as frame_support::traits::Hooks<u64>>::on_initialize(10);
    let assignment = Workers::assign_work(77, 0).unwrap().into_inner();
    assert_ok!(Workers::open_voting(77, 0, [7; 32], 15));
    (pairs, assignment)
}

fn signed_vote(pair: &sr25519::Pair, worker_id: u64, verdict: Verdict) {
    let vote = WorkerVoteV1 {
        work_id: 77,
        round: 0,
        assignment_epoch: 1,
        candidate_report_hash: [7; 32],
        verdict,
        deadline: 15,
        chain_id: [42; 32],
        protocol_version: PROTOCOL_VERSION_V1,
    };
    let signature = pair.sign(&vote.signing_hash()).0;
    assert_ok!(Workers::submit_vote(
        RuntimeOrigin::signed(99),
        worker_id,
        vote,
        signature
    ));
}

#[test]
fn threshold_locks_but_round_waits_for_all_workers() {
    new_test_ext().execute_with(|| {
        let (pairs, assignment) = setup_signed_voting();
        signed_vote(
            &pairs[assignment[0] as usize],
            assignment[0],
            Verdict::Support,
        );
        signed_vote(
            &pairs[assignment[1] as usize],
            assignment[1],
            Verdict::Support,
        );
        assert!(Workers::round_result((77, 0)).is_none());

        signed_vote(
            &pairs[assignment[2] as usize],
            assignment[2],
            Verdict::Oppose(minijam_protocol::OpposeReason::MissingData),
        );
        let result = Workers::round_result((77, 0)).unwrap();
        assert_eq!(
            result.decision,
            Some(pallet_minijam_workers::RoundDecision::Accepted)
        );
        assert!(result.absentees.is_empty());
        for worker_id in assignment {
            assert_eq!(
                Balances::total_balance(&(worker_id + 1)),
                10_011,
                "Support and Oppose must receive the same response reward"
            );
        }
    });
}

#[test]
fn deadline_finalizes_and_records_only_missing_workers() {
    new_test_ext().execute_with(|| {
        let (pairs, assignment) = setup_signed_voting();
        signed_vote(
            &pairs[assignment[0] as usize],
            assignment[0],
            Verdict::Support,
        );
        System::set_block_number(16);
        <Workers as frame_support::traits::Hooks<u64>>::on_initialize(16);

        let result = Workers::round_result((77, 0)).unwrap();
        assert_eq!(result.decision, None);
        assert_eq!(result.absentees.len(), 2);
        assert!(!result.absentees.contains(&assignment[0]));

        assert_eq!(Balances::free_balance(100), 1_000_011);
        assert_eq!(
            pallet_minijam_workers::Workers::<Test>::get(assignment[0])
                .unwrap()
                .active_stake,
            1_000 + assignment[0] as u128
        );
        for absent in result.absentees {
            assert_eq!(
                pallet_minijam_workers::Workers::<Test>::get(absent)
                    .unwrap()
                    .active_stake,
                990 + absent as u128
            );
        }
    });
}
