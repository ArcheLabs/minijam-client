use crate as pallet_minijam;
use frame_support::{
    assert_noop, assert_ok, derive_impl, parameter_types,
    traits::tokens::fungible::{Inspect, InspectHold},
};
use minijam_jamcore_api::{
    InputError, MiniJamError, MiniJamExecutionInputV1, MiniJamExecutionInputV2,
    MiniJamExecutionOutputV1, MiniJamExecutionOutputV2, MiniJamExecutor, ProtocolStateReader,
};
use minijam_protocol::{
    blake2_256, BulletinEvidence, CanonicalReportBytes, PreimageMetadataV1, ProtocolStateChange,
    ReportEnvelopeV1, ReportMetadataV1, ReportSignatures, StateOperation, StateValue,
    SystemCommandV1, Verdict, WorkerVoteV1, NS_SERVICE_STORAGE, PROTOCOL_VERSION_V1,
};
use parity_scale_codec::Encode;
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

    #[runtime::pallet_index(3)]
    pub type MiniJam = pallet_minijam::Pallet<Test>;
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
    pub const EquivocationSlash: sp_runtime::Perbill = sp_runtime::Perbill::from_percent(20);
    pub const WorkDeposit: u128 = 100;
    pub const CandidateBond: u128 = 100;
    pub const CandidateRejectionSlash: u128 = 10;
    pub const AcceptedSubmitterReward: u128 = 10;
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
    type EpochLength = frame_support::traits::ConstU32<100>;
    type MaxCandidates = frame_support::traits::ConstU32<8>;
    type TopWorkers = frame_support::traits::ConstU32<8>;
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
    type EquivocationSlash = EquivocationSlash;
    type EquivocationSuspension = frame_support::traits::ConstU32<2>;
}

impl pallet_minijam::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type JamHoldReason = RuntimeHoldReason;
    type ChainId = ChainId;
    type WorkDeposit = WorkDeposit;
    type CandidateBond = CandidateBond;
    type CandidateRejectionSlash = CandidateRejectionSlash;
    type AcceptedSubmitterReward = AcceptedSubmitterReward;
    type RewardPool = RewardPool;
    type ReportSubmissionDeadline = frame_support::traits::ConstU32<20>;
    type VoteWindow = frame_support::traits::ConstU32<10>;
    type MaxCandidateRounds = frame_support::traits::ConstU8<3>;
    type MaxPendingWorks = frame_support::traits::ConstU32<8>;
    type MaxExecutionReports = frame_support::traits::ConstU32<4>;
    type MaxExecutionGas = frame_support::traits::ConstU64<1_000_000>;
    type JamCoreExecutor = TestExecutor;
    type MaxPendingPreimages = frame_support::traits::ConstU32<8>;
    type MaxPendingSystemOps = frame_support::traits::ConstU32<8>;
}

#[derive(Default)]
pub struct TestExecutor;

impl MiniJamExecutor for TestExecutor {
    fn execute<R: ProtocolStateReader>(
        &self,
        input: MiniJamExecutionInputV1,
        _state: &R,
    ) -> Result<MiniJamExecutionOutputV1, MiniJamError> {
        let mut output = MiniJamExecutionOutputV1::empty();
        output.consumed_reports = input
            .reports
            .iter()
            .map(|report| blake2_256(report))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        output.consumed_preimages = input
            .preimages
            .iter()
            .map(|preimage| blake2_256(preimage))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        output.header_hash = [1; 32];
        output.accumulate_root = [2; 32];
        if !input.reports.is_empty() {
            let mut key = [0u8; 31];
            key[0] = NS_SERVICE_STORAGE;
            key[30] = 7;
            output
                .ordered_changes
                .try_push(ProtocolStateChange {
                    key,
                    operation: StateOperation::Upsert,
                    value: Some(StateValue::try_from(vec![input.reports.len() as u8]).unwrap()),
                })
                .unwrap();
        }
        output.gas_used = 10;
        output.receipt_hash = output.compute_receipt_hash();
        Ok(output)
    }

    fn execute_v2<R: ProtocolStateReader>(
        &self,
        input: MiniJamExecutionInputV2,
        _state: &R,
    ) -> Result<MiniJamExecutionOutputV2, MiniJamError> {
        let mut output = MiniJamExecutionOutputV2::empty();
        output.consumed_reports = input
            .reports
            .iter()
            .map(|report| blake2_256(report))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        output.consumed_preimages = input
            .preimages
            .iter()
            .map(|preimage| blake2_256(preimage))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        output.consumed_system_ops = input
            .system_ops
            .iter()
            .map(|op| op.request_id)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        output.input_hash = blake2_256(&input.encode());
        output.header_hash = [1; 32];
        output.accumulate_root = [2; 32];
        if !input.reports.is_empty() {
            let mut key = [0u8; 31];
            key[0] = NS_SERVICE_STORAGE;
            key[30] = 7;
            output
                .ordered_changes
                .try_push(ProtocolStateChange {
                    key,
                    operation: StateOperation::Upsert,
                    value: Some(StateValue::try_from(vec![input.reports.len() as u8]).unwrap()),
                })
                .unwrap();
        }
        output.gas_used = 10;
        output.receipt_hash = output.compute_receipt_hash();
        Ok(output)
    }

    fn validate_preimage_submission<R: ProtocolStateReader>(
        &self,
        bytes: &[u8],
        _state: &R,
    ) -> Result<PreimageMetadataV1, MiniJamError> {
        if bytes.is_empty() {
            return Err(MiniJamError::Input(InputError::InvalidPreimageEncoding));
        }
        Ok(PreimageMetadataV1 {
            requester: u32::from(bytes[0]),
            blob_hash: blake2_256(bytes),
            blob_len: bytes.len() as u32,
        })
    }
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
            (5, 10_001),
            (6, 10_001),
            (100, 1_000_001),
        ],
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .unwrap();
    storage.into()
}

fn activate_workers() -> Vec<sr25519::Pair> {
    let pairs: Vec<_> = (1u8..=3)
        .map(|seed| sr25519::Pair::from_seed(&[seed; 32]))
        .collect();
    for (index, pair) in pairs.iter().enumerate() {
        assert_ok!(Workers::register(
            RuntimeOrigin::signed((index + 1) as u64),
            pair.public().0,
            1_000
        ));
    }
    System::set_block_number(100);
    <Workers as frame_support::traits::Hooks<u64>>::on_initialize(100);
    pairs
}

fn envelope(work_id: u64, round: u8) -> ReportEnvelopeV1 {
    let canonical_report = CanonicalReportBytes::try_from(vec![1, 2, 3, round]).unwrap();
    ReportEnvelopeV1 {
        protocol_version: PROTOCOL_VERSION_V1,
        chain_id: [42; 32],
        work_id,
        assignment_round: round,
        canonical_report_hash: blake2_256(&canonical_report),
        canonical_report,
        projected_metadata: ReportMetadataV1 {
            package_hash: [1; 32],
            context_hash: [2; 32],
            exports_root: [3; 32],
            accumulate_gas: 100,
        },
        bulletin_evidence: BulletinEvidence::NoExternalProofV1 { receipt: None },
        signatures: ReportSignatures::default(),
    }
}

fn vote(pair: &sr25519::Pair, worker_id: u64, verdict: Verdict) {
    let vote = WorkerVoteV1 {
        work_id: 0,
        round: 0,
        assignment_epoch: 1,
        candidate_report_hash: envelope(0, 0).canonical_report_hash,
        verdict,
        deadline: 110,
        chain_id: [42; 32],
        protocol_version: PROTOCOL_VERSION_V1,
    };
    let signature = pair.sign(&vote.signing_hash()).0;
    assert_ok!(Workers::submit_vote(
        RuntimeOrigin::signed(55),
        worker_id,
        vote,
        signature
    ));
}

#[test]
fn accepted_candidate_releases_bonds_and_enters_execution_queue() {
    new_test_ext().execute_with(|| {
        let pairs = activate_workers();
        assert_ok!(MiniJam::submit_work(RuntimeOrigin::signed(5)));
        assert_eq!(
            MiniJam::work(0).unwrap().status,
            pallet_minijam::WorkStatus::AwaitingCandidate
        );
        assert_ok!(MiniJam::submit_candidate(
            RuntimeOrigin::signed(6),
            Box::new(envelope(0, 0))
        ));
        let assignment = Workers::assignment(0, 0).unwrap();
        vote(
            &pairs[assignment[0] as usize],
            assignment[0],
            Verdict::Support,
        );
        vote(
            &pairs[assignment[1] as usize],
            assignment[1],
            Verdict::Support,
        );
        vote(
            &pairs[assignment[2] as usize],
            assignment[2],
            Verdict::Oppose(minijam_protocol::OpposeReason::MissingData),
        );
        <MiniJam as frame_support::traits::Hooks<u64>>::on_initialize(100);

        assert_eq!(
            MiniJam::work(0).unwrap().status,
            pallet_minijam::WorkStatus::Accepted
        );
        assert_eq!(
            pallet_minijam::ExecutionQueue::<Test>::get()
                .iter()
                .map(|item| item.work_id)
                .collect::<Vec<_>>(),
            vec![0]
        );
        let work_reason: RuntimeHoldReason = pallet_minijam::HoldReason::WorkDeposit.into();
        let candidate_reason: RuntimeHoldReason = pallet_minijam::HoldReason::CandidateBond.into();
        assert_eq!(Balances::balance_on_hold(&work_reason, &5), 0);
        assert_eq!(Balances::balance_on_hold(&candidate_reason, &6), 0);
        assert_eq!(Balances::total_balance(&6), 10_011);
    });
}

#[test]
fn accepted_candidate_executes_next_block_and_commits_delta() {
    new_test_ext().execute_with(|| {
        let pairs = activate_workers();
        assert_ok!(MiniJam::submit_work(RuntimeOrigin::signed(5)));
        assert_ok!(MiniJam::submit_candidate(
            RuntimeOrigin::signed(6),
            Box::new(envelope(0, 0))
        ));
        let assignment = Workers::assignment(0, 0).unwrap();
        vote(
            &pairs[assignment[0] as usize],
            assignment[0],
            Verdict::Support,
        );
        vote(
            &pairs[assignment[1] as usize],
            assignment[1],
            Verdict::Support,
        );
        vote(
            &pairs[assignment[2] as usize],
            assignment[2],
            Verdict::Oppose(minijam_protocol::OpposeReason::MissingData),
        );
        <MiniJam as frame_support::traits::Hooks<u64>>::on_initialize(100);

        <MiniJam as frame_support::traits::Hooks<u64>>::on_finalize(100);
        assert_eq!(
            MiniJam::work(0).unwrap().status,
            pallet_minijam::WorkStatus::Accepted
        );
        assert_eq!(pallet_minijam::ExecutionQueue::<Test>::get().len(), 1);

        System::set_block_number(101);
        <MiniJam as frame_support::traits::Hooks<u64>>::on_finalize(101);

        let mut key = [0u8; 31];
        key[0] = NS_SERVICE_STORAGE;
        key[30] = 7;
        assert_eq!(
            pallet_minijam::ProtocolState::<Test>::get(key)
                .unwrap()
                .into_inner(),
            vec![1]
        );
        assert_eq!(
            MiniJam::work(0).unwrap().status,
            pallet_minijam::WorkStatus::Imported
        );
        assert!(pallet_minijam::ExecutionQueue::<Test>::get().is_empty());
        assert!(pallet_minijam::ExecutionReceipts::<Test>::get(0).is_some());
        assert!(pallet_minijam::LastExecutionReceipt::<Test>::get().is_some());
    });
}

#[test]
fn queued_preimages_are_imported_with_next_virtual_block() {
    new_test_ext().execute_with(|| {
        let preimage = minijam_protocol::CanonicalPreimageBytes::try_from(vec![7, 8, 9]).unwrap();
        let canonical_hash = blake2_256(&preimage);
        assert_ok!(MiniJam::submit_preimage(RuntimeOrigin::signed(6), preimage));
        assert_eq!(pallet_minijam::PendingPreimages::<Test>::get().len(), 1);

        System::set_block_number(100);
        <MiniJam as frame_support::traits::Hooks<u64>>::on_finalize(100);

        assert!(pallet_minijam::PendingPreimages::<Test>::get().is_empty());
        assert!(!pallet_minijam::PendingPreimageKeys::<Test>::contains_key(
            pallet_minijam::PreimageKeyV1 {
                requester: 7,
                blob_hash: canonical_hash,
                blob_len: 3
            }
        ));
    });
}

#[test]
fn queued_system_ops_are_consumed_with_next_virtual_block() {
    new_test_ext().execute_with(|| {
        let command = SystemCommandV1::CreateService {
            code_hash: [9u8; 32],
            code_len: 32,
            min_item_gas: 1,
            min_memo_gas: 1,
            initial_balance: 100,
        };
        assert_ok!(MiniJam::submit_system_op(
            RuntimeOrigin::signed(5),
            Box::new(command)
        ));
        let pending = pallet_minijam::PendingSystemOps::<Test>::get();
        assert_eq!(pending.len(), 1);
        let request_id = pending[0].op.request_id;
        assert!(pallet_minijam::PendingSystemOpKeys::<Test>::contains_key(
            request_id
        ));

        System::set_block_number(100);
        <MiniJam as frame_support::traits::Hooks<u64>>::on_finalize(100);

        assert!(pallet_minijam::PendingSystemOps::<Test>::get().is_empty());
        assert!(!pallet_minijam::PendingSystemOpKeys::<Test>::contains_key(
            request_id
        ));
    });
}

#[test]
fn invalid_system_ops_are_rejected_before_queueing() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            MiniJam::submit_system_op(
                RuntimeOrigin::signed(5),
                Box::new(SystemCommandV1::CreateService {
                    code_hash: [9u8; 32],
                    code_len: 0,
                    min_item_gas: 1,
                    min_memo_gas: 1,
                    initial_balance: 100,
                })
            ),
            pallet_minijam::Error::<Test>::InvalidSystemOp
        );
        assert!(pallet_minijam::PendingSystemOps::<Test>::get().is_empty());
    });
}

#[test]
fn empty_block_executes_stf() {
    new_test_ext().execute_with(|| {
        assert!(pallet_minijam::LastExecutionReceipt::<Test>::get().is_none());

        System::set_block_number(42);
        <MiniJam as frame_support::traits::Hooks<u64>>::on_finalize(42);

        assert!(pallet_minijam::LastExecutionReceipt::<Test>::get().is_some());
    });
}

#[test]
fn root_pause_delays_imports_but_still_executes_empty_stf() {
    new_test_ext().execute_with(|| {
        let pairs = activate_workers();
        assert_ok!(MiniJam::submit_work(RuntimeOrigin::signed(5)));
        assert_ok!(MiniJam::submit_candidate(
            RuntimeOrigin::signed(6),
            Box::new(envelope(0, 0))
        ));
        let assignment = Workers::assignment(0, 0).unwrap();
        vote(
            &pairs[assignment[0] as usize],
            assignment[0],
            Verdict::Support,
        );
        vote(
            &pairs[assignment[1] as usize],
            assignment[1],
            Verdict::Support,
        );
        vote(
            &pairs[assignment[2] as usize],
            assignment[2],
            Verdict::Oppose(minijam_protocol::OpposeReason::MissingData),
        );
        <MiniJam as frame_support::traits::Hooks<u64>>::on_initialize(100);

        assert_ok!(MiniJam::pause_execution(RuntimeOrigin::root(), true));
        System::set_block_number(101);
        <MiniJam as frame_support::traits::Hooks<u64>>::on_finalize(101);
        assert_eq!(
            MiniJam::work(0).unwrap().status,
            pallet_minijam::WorkStatus::Accepted
        );
        assert_eq!(pallet_minijam::ExecutionQueue::<Test>::get().len(), 1);
        assert!(pallet_minijam::LastExecutionReceipt::<Test>::get().is_some());

        assert_ok!(MiniJam::pause_execution(RuntimeOrigin::root(), false));
        <MiniJam as frame_support::traits::Hooks<u64>>::on_finalize(101);
        assert_eq!(
            MiniJam::work(0).unwrap().status,
            pallet_minijam::WorkStatus::Imported
        );
        assert!(pallet_minijam::ExecutionQueue::<Test>::get().is_empty());
    });
}

#[test]
fn root_quarantine_moves_execution_queue_without_executing() {
    new_test_ext().execute_with(|| {
        let pairs = activate_workers();
        assert_ok!(MiniJam::submit_work(RuntimeOrigin::signed(5)));
        assert_ok!(MiniJam::submit_candidate(
            RuntimeOrigin::signed(6),
            Box::new(envelope(0, 0))
        ));
        let assignment = Workers::assignment(0, 0).unwrap();
        vote(
            &pairs[assignment[0] as usize],
            assignment[0],
            Verdict::Support,
        );
        vote(
            &pairs[assignment[1] as usize],
            assignment[1],
            Verdict::Support,
        );
        vote(
            &pairs[assignment[2] as usize],
            assignment[2],
            Verdict::Oppose(minijam_protocol::OpposeReason::MissingData),
        );
        <MiniJam as frame_support::traits::Hooks<u64>>::on_initialize(100);

        assert_ok!(MiniJam::quarantine_pending(RuntimeOrigin::root()));
        assert!(pallet_minijam::ExecutionQueue::<Test>::get().is_empty());
        assert_eq!(
            pallet_minijam::QuarantinedExecutionQueue::<Test>::get()
                .iter()
                .map(|item| item.work_id)
                .collect::<Vec<_>>(),
            vec![0]
        );

        System::set_block_number(101);
        <MiniJam as frame_support::traits::Hooks<u64>>::on_finalize(101);
        assert_eq!(
            MiniJam::work(0).unwrap().status,
            pallet_minijam::WorkStatus::Accepted
        );
    });
}

#[test]
fn rejected_candidate_is_slashed_and_advances_round() {
    new_test_ext().execute_with(|| {
        let pairs = activate_workers();
        assert_ok!(MiniJam::submit_work(RuntimeOrigin::signed(5)));
        assert_ok!(MiniJam::submit_candidate(
            RuntimeOrigin::signed(6),
            Box::new(envelope(0, 0))
        ));
        let assignment = Workers::assignment(0, 0).unwrap();
        vote(
            &pairs[assignment[0] as usize],
            assignment[0],
            Verdict::Oppose(minijam_protocol::OpposeReason::InvalidRefine),
        );
        vote(
            &pairs[assignment[1] as usize],
            assignment[1],
            Verdict::Oppose(minijam_protocol::OpposeReason::InvalidRefine),
        );
        vote(
            &pairs[assignment[2] as usize],
            assignment[2],
            Verdict::Support,
        );
        <MiniJam as frame_support::traits::Hooks<u64>>::on_initialize(100);

        let work = MiniJam::work(0).unwrap();
        assert_eq!(work.round, 1);
        assert_eq!(work.status, pallet_minijam::WorkStatus::AwaitingCandidate);
        assert_eq!(Balances::total_balance(&6), 9_991);
    });
}

#[test]
fn missing_candidate_advances_without_penalizing_workers() {
    new_test_ext().execute_with(|| {
        activate_workers();
        assert_ok!(MiniJam::submit_work(RuntimeOrigin::signed(5)));
        System::set_block_number(121);
        <MiniJam as frame_support::traits::Hooks<u64>>::on_initialize(121);
        let work = MiniJam::work(0).unwrap();
        assert_eq!(work.round, 1);
        assert_eq!(work.status, pallet_minijam::WorkStatus::AwaitingCandidate);
        for worker_id in 0..3 {
            assert_eq!(
                pallet_minijam_workers::Workers::<Test>::get(worker_id)
                    .unwrap()
                    .active_stake,
                1_000
            );
        }
    });
}

#[test]
fn work_remains_insufficient_when_k_is_unavailable() {
    new_test_ext().execute_with(|| {
        assert_ok!(MiniJam::submit_work(RuntimeOrigin::signed(5)));
        assert_eq!(
            MiniJam::work(0).unwrap().status,
            pallet_minijam::WorkStatus::InsufficientWorkers
        );
    });
}

#[test]
fn three_missing_candidate_rounds_fail_and_slash_work_deposit() {
    new_test_ext().execute_with(|| {
        activate_workers();
        let pool_before = Balances::total_balance(&100);
        assert_ok!(MiniJam::submit_work(RuntimeOrigin::signed(5)));
        for block in [121, 142, 163] {
            System::set_block_number(block);
            <MiniJam as frame_support::traits::Hooks<u64>>::on_initialize(block);
        }

        assert_eq!(
            MiniJam::work(0).unwrap().status,
            pallet_minijam::WorkStatus::Failed
        );
        assert!(pallet_minijam::PendingWorks::<Test>::get().is_empty());
        assert_eq!(Balances::total_balance(&100), pool_before + 100);
    });
}
