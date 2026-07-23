use crate as pallet_minijam;
use frame_support::{
    assert_noop, assert_ok, derive_impl, parameter_types,
    traits::tokens::fungible::{Inspect, InspectHold},
};
use jam_codec::{Decode as JamDecode, Encode as JamEncode};
use jp_core_primitives::{
    crypto::OpaqueHash,
    simple::{ByteSequence, TimeSlot},
    state::StoreKey,
    work::{
        RefineContext, RefineLoad, WorkExecResult, WorkItem, WorkPackage, WorkPackageSpec,
        WorkReport, WorkResult,
    },
};
use minijam_jamcore_api::{
    ExecutionOutcome, InputError, MiniJamError, MiniJamExecutionInputV1, MiniJamExecutionInputV2,
    MiniJamExecutionOutputV1, MiniJamExecutionOutputV2, MiniJamExecutor, ProtocolStateReader,
    ReportProjectionV1, ServiceResultProjection,
};
use minijam_protocol::{
    blake2_256, BulletinEvidence, CanonicalReportBytes, ContentRef, PreimageMetadataV1,
    ProtocolStateChange, ReportEnvelopeV1, ReportMetadataV1, ReportSignatures, StateOperation,
    StateValue, SystemCommandV1, Verdict, WorkerVoteV1, NS_SERVICE_STORAGE, PROTOCOL_VERSION_V1,
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
    pub const FuelEscrow: u64 = 101;
    pub const TimelyVoteReward: u128 = 10;
    pub const MinimumAbsenceSlash: u128 = 10;
    pub const AbsenceSlash: sp_runtime::Perbill = sp_runtime::Perbill::from_percent(1);
    pub const EquivocationSlash: sp_runtime::Perbill = sp_runtime::Perbill::from_percent(20);
    pub const WorkDeposit: u128 = 100;
    pub const CandidateBond: u128 = 100;
    pub const CandidateRejectionSlash: u128 = 10;
    pub const AcceptedSubmitterReward: u128 = 10;
    pub const RefineGasPrice: u128 = 1;
    pub const AccumulateGasPrice: u128 = 1;
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
    type FuelEscrowAccount = FuelEscrow;
    type RefineGasPrice = RefineGasPrice;
    type AccumulateGasPrice = AccumulateGasPrice;
    type ReportSubmissionDeadline = frame_support::traits::ConstU32<20>;
    type VoteWindow = frame_support::traits::ConstU32<10>;
    type MaxCandidateRounds = frame_support::traits::ConstU8<3>;
    type MaxPendingWorks = frame_support::traits::ConstU32<8>;
    type MaxExecutionReports = frame_support::traits::ConstU32<4>;
    type MaxExecutionGas = frame_support::traits::ConstU64<1_000_000>;
    type MaxWorkPackageBytes = frame_support::traits::ConstU32<512>;
    type MaxBundleBytes = frame_support::traits::ConstU64<128>;
    type MaxServicesPerWork = frame_support::traits::ConstU32<8>;
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
        if input.system_ops.iter().any(|op| {
            matches!(
                op.command,
                SystemCommandV1::CreateService { code_hash, .. } if code_hash == [0xee; 32]
            )
        }) {
            return Err(MiniJamError::Execution(ExecutionOutcome::ServiceFailure));
        }

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

    fn project_report(&self, bytes: &[u8]) -> Result<ReportProjectionV1, MiniJamError> {
        let mut input = bytes;
        let report = WorkReport::decode(&mut input)
            .map_err(|_| MiniJamError::Input(InputError::InvalidReportEncoding))?;
        if !input.is_empty() {
            return Err(MiniJamError::Input(InputError::InvalidReportEncoding));
        }

        let mut total_refine_gas = 0u64;
        let mut total_accumulate_gas = 0u64;
        let mut services = Vec::new();
        for result in &report.results {
            total_refine_gas = total_refine_gas
                .checked_add(result.refine_load.gas_used)
                .ok_or(MiniJamError::Input(InputError::LimitExceeded))?;
            total_accumulate_gas = total_accumulate_gas
                .checked_add(result.accumulate_gas)
                .ok_or(MiniJamError::Input(InputError::LimitExceeded))?;
            services.push(ServiceResultProjection {
                service_id: result.service_id,
                code_hash: result.code_hash.0,
                refine_gas_used: result.refine_load.gas_used,
                accumulate_gas: result.accumulate_gas,
            });
        }

        Ok(ReportProjectionV1 {
            package_hash: report.package_spec.hash.0,
            context_hash: blake2_256(&JamEncode::encode(&report.context)),
            exports_root: report.package_spec.exports_root.0,
            result_count: report.results.len() as u32,
            services,
            total_refine_gas,
            total_accumulate_gas,
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

fn test_ext_with_protocol_state(
    protocol_state: Vec<(Vec<u8>, Vec<u8>)>,
) -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    pallet_minijam::GenesisConfig::<Test> {
        protocol_state,
        service_fuel: Vec::new(),
        _phantom: Default::default(),
    }
    .assimilate_storage(&mut storage)
    .unwrap();
    storage.into()
}

fn service_info_key(service_id: u32) -> [u8; 31] {
    let service = service_id.to_le_bytes();
    let mut key = [0u8; 31];
    key[0] = 0xff;
    key[1] = service[0];
    key[3] = service[1];
    key[5] = service[2];
    key[7] = service[3];
    key
}

fn system_receipt_key(request_id: &[u8; 32]) -> [u8; 31] {
    let mut storage_key = b"system/receipt/".to_vec();
    storage_key.extend_from_slice(request_id);
    StoreKey::new_service_storage_key(&0, &ByteSequence::from(storage_key))
        .to_state_key()
        .0
}

fn work_package(
    seed: u8,
) -> frame_support::BoundedVec<u8, <Test as pallet_minijam::Config>::MaxWorkPackageBytes> {
    encoded_work_package(seed, Vec::new())
}

fn work_item(service: u32, refine_gas_limit: u64, accumulate_gas_limit: u64) -> WorkItem {
    WorkItem {
        service,
        code_hash: OpaqueHash([9u8; 32]),
        refine_gas_limit,
        accumulate_gas_limit,
        export_count: 0,
        payload: ByteSequence::from(Vec::new()),
        import_segments: Vec::new(),
        extrinsic: Vec::new(),
    }
}

fn encoded_work_package(
    seed: u8,
    items: Vec<WorkItem>,
) -> frame_support::BoundedVec<u8, <Test as pallet_minijam::Config>::MaxWorkPackageBytes> {
    let package = WorkPackage {
        auth_code_host: 0,
        auth_code_hash: OpaqueHash([seed; 32]),
        context: RefineContext {
            anchor: OpaqueHash([1u8; 32]),
            state_root: OpaqueHash([2u8; 32]),
            beefy_root: OpaqueHash([3u8; 32]),
            lookup_anchor: OpaqueHash([4u8; 32]),
            lookup_anchor_slot: TimeSlot(5),
            prerequisites: Vec::new(),
        },
        authorization: ByteSequence::from(Vec::new()),
        authorizer_config: ByteSequence::from(Vec::new()),
        items,
    };
    JamEncode::encode(&package).try_into().unwrap()
}

fn bundle_ref(seed: u8) -> ContentRef {
    ContentRef {
        cid_v1: vec![seed].try_into().unwrap(),
        content_hash: [seed; 32],
        size: 1,
    }
}

fn submit_work(owner: u64) -> frame_support::dispatch::DispatchResult {
    MiniJam::submit_work(
        RuntimeOrigin::signed(owner),
        work_package(owner as u8),
        bundle_ref(owner as u8),
    )
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
    let package_hash = MiniJam::work(work_id).unwrap().package_hash;
    let canonical_report = encoded_work_report(package_hash, Vec::new());
    envelope_with_report(work_id, round, canonical_report)
}

fn envelope_with_report(
    work_id: u64,
    round: u8,
    canonical_report: CanonicalReportBytes,
) -> ReportEnvelopeV1 {
    let projection = TestExecutor
        .project_report(&canonical_report)
        .expect("test report must project");
    ReportEnvelopeV1 {
        protocol_version: PROTOCOL_VERSION_V1,
        chain_id: [42; 32],
        work_id,
        assignment_round: round,
        canonical_report_hash: blake2_256(&canonical_report),
        canonical_report,
        projected_metadata: ReportMetadataV1 {
            package_hash: projection.package_hash,
            context_hash: projection.context_hash,
            exports_root: projection.exports_root,
            accumulate_gas: projection.total_accumulate_gas,
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

fn vote_for_report(
    pair: &sr25519::Pair,
    worker_id: u64,
    candidate_report_hash: [u8; 32],
    verdict: Verdict,
) {
    let vote = WorkerVoteV1 {
        work_id: 0,
        round: 0,
        assignment_epoch: 1,
        candidate_report_hash,
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

fn encoded_work_report(package_hash: [u8; 32], results: Vec<WorkResult>) -> CanonicalReportBytes {
    let report = WorkReport {
        package_spec: WorkPackageSpec {
            hash: OpaqueHash(package_hash),
            length: 1,
            erasure_root: OpaqueHash([5u8; 32]),
            exports_root: OpaqueHash([6u8; 32]),
            exports_count: 0,
        },
        context: RefineContext {
            anchor: OpaqueHash([1u8; 32]),
            state_root: OpaqueHash([2u8; 32]),
            beefy_root: OpaqueHash([3u8; 32]),
            lookup_anchor: OpaqueHash([4u8; 32]),
            lookup_anchor_slot: TimeSlot(5),
            prerequisites: Vec::new(),
        },
        core_index: 0,
        authorizer_hash: OpaqueHash([7u8; 32]),
        auth_gas_used: 0,
        auth_output: ByteSequence::from(Vec::new()),
        segment_root_lookup: Vec::new(),
        results,
    };
    CanonicalReportBytes::try_from(JamEncode::encode(&report)).unwrap()
}

fn work_result(service_id: u32, refine_gas_used: u64, accumulate_gas: u64) -> WorkResult {
    WorkResult {
        service_id,
        code_hash: OpaqueHash([9u8; 32]),
        payload_hash: OpaqueHash([8u8; 32]),
        accumulate_gas,
        result: WorkExecResult::Ok(ByteSequence::from(Vec::new())),
        refine_load: RefineLoad {
            gas_used: refine_gas_used,
            imports: 0,
            extrinsic_count: 0,
            extrinsic_size: 0,
            exports: 0,
        },
    }
}

fn work_result_with_code(
    service_id: u32,
    code_hash: [u8; 32],
    refine_gas_used: u64,
    accumulate_gas: u64,
) -> WorkResult {
    let mut result = work_result(service_id, refine_gas_used, accumulate_gas);
    result.code_hash = OpaqueHash(code_hash);
    result
}

fn submit_assigned_service_work() -> [u8; 32] {
    activate_workers();
    pallet_minijam::ProtocolState::<Test>::insert(
        service_info_key(7),
        StateValue::try_from(vec![1, 2, 3]).unwrap(),
    );
    assert_ok!(MiniJam::fund_service(RuntimeOrigin::signed(5), 7, 100));
    let package = encoded_work_package(31, vec![work_item(7, 10, 20)]);
    let package_hash = blake2_256(&package);
    assert_ok!(MiniJam::submit_work(
        RuntimeOrigin::signed(5),
        package,
        bundle_ref(31)
    ));
    package_hash
}

#[test]
fn submit_work_stores_package_hash_and_bundle_ref() {
    new_test_ext().execute_with(|| {
        let package = work_package(11);
        let bundle = bundle_ref(12);
        let package_hash = blake2_256(&package);

        assert_ok!(MiniJam::submit_work(
            RuntimeOrigin::signed(5),
            package.clone(),
            bundle.clone()
        ));

        let work = MiniJam::work(0).unwrap();
        assert_eq!(work.owner, 5);
        assert_eq!(work.package_hash, package_hash);
        assert_eq!(work.canonical_work_package, package);
        assert_eq!(work.bundle_ref, bundle);
        assert_eq!(
            pallet_minijam::WorkByPackageHash::<Test>::get(package_hash),
            Some(0)
        );
        let queried = MiniJam::get_work(0).unwrap();
        assert_eq!(queried.owner, work.owner);
        assert_eq!(queried.package_hash, work.package_hash);
        let queried_by_hash = MiniJam::get_work_by_package_hash(package_hash).unwrap();
        assert_eq!(queried_by_hash.owner, work.owner);
        assert_eq!(queried_by_hash.package_hash, work.package_hash);
        assert_eq!(MiniJam::get_work_bundle_ref(0), Some(bundle));
    });
}

#[test]
fn pending_worker_tasks_project_finalized_worker_inputs() {
    new_test_ext().execute_with(|| {
        activate_workers();
        let package = work_package(21);
        let bundle = bundle_ref(22);
        let package_hash = blake2_256(&package);

        assert_ok!(MiniJam::submit_work(
            RuntimeOrigin::signed(5),
            package.clone(),
            bundle.clone()
        ));

        let tasks = MiniJam::pending_worker_tasks();

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].work_id, 0);
        assert_eq!(tasks[0].round, 0);
        assert_eq!(tasks[0].package_hash, package_hash);
        assert_eq!(
            tasks[0].canonical_work_package.as_slice(),
            package.as_slice()
        );
        assert_eq!(tasks[0].bundle_ref, bundle);
    });
}

#[test]
fn submit_candidate_rejects_report_not_bound_to_work_package() {
    new_test_ext().execute_with(|| {
        let package_hash = submit_assigned_service_work();
        let cases = [
            encoded_work_report([0x44; 32], vec![work_result(7, 4, 6)]),
            encoded_work_report(package_hash, Vec::new()),
            encoded_work_report(package_hash, vec![work_result(7, 11, 6)]),
            encoded_work_report(package_hash, vec![work_result(7, 4, 21)]),
            encoded_work_report(package_hash, vec![work_result(8, 4, 6)]),
            encoded_work_report(
                package_hash,
                vec![work_result_with_code(7, [0xaa; 32], 4, 6)],
            ),
        ];

        for report in cases {
            assert_noop!(
                MiniJam::submit_candidate(
                    RuntimeOrigin::signed(6),
                    Box::new(envelope_with_report(0, 0, report))
                ),
                pallet_minijam::Error::<Test>::InvalidReportProjection
            );
            assert!(pallet_minijam::Candidates::<Test>::get(0, 0).is_none());
        }

        let mut trailing = envelope_with_report(
            0,
            0,
            encoded_work_report(package_hash, vec![work_result(7, 4, 6)]),
        );
        trailing.canonical_report.try_push(0).unwrap();
        trailing.canonical_report_hash = blake2_256(&trailing.canonical_report);
        assert_noop!(
            MiniJam::submit_candidate(RuntimeOrigin::signed(6), Box::new(trailing)),
            pallet_minijam::Error::<Test>::InvalidReportProjection
        );
        assert!(pallet_minijam::Candidates::<Test>::get(0, 0).is_none());
    });
}

#[test]
fn submit_work_rejects_duplicate_package_and_invalid_bundle() {
    new_test_ext().execute_with(|| {
        let package = work_package(11);
        let bundle = bundle_ref(12);

        assert_ok!(MiniJam::submit_work(
            RuntimeOrigin::signed(5),
            package.clone(),
            bundle.clone()
        ));
        assert_noop!(
            MiniJam::submit_work(RuntimeOrigin::signed(6), package, bundle),
            pallet_minijam::Error::<Test>::DuplicateWorkPackage
        );
        assert_noop!(
            MiniJam::submit_work(
                RuntimeOrigin::signed(6),
                work_package(13),
                ContentRef {
                    cid_v1: Default::default(),
                    content_hash: [0u8; 32],
                    size: 1,
                }
            ),
            pallet_minijam::Error::<Test>::InvalidContentRef
        );
        assert_noop!(
            MiniJam::submit_work(
                RuntimeOrigin::signed(6),
                work_package(14),
                ContentRef {
                    cid_v1: vec![1].try_into().unwrap(),
                    content_hash: [0u8; 32],
                    size: 129,
                }
            ),
            pallet_minijam::Error::<Test>::InvalidContentRef
        );
    });
}

#[test]
fn genesis_config_seeds_protocol_state() {
    let key = service_info_key(0);
    test_ext_with_protocol_state(vec![(key.to_vec(), vec![1, 2, 3])]).execute_with(|| {
        assert_eq!(
            pallet_minijam::ProtocolState::<Test>::get(key)
                .unwrap()
                .into_inner(),
            vec![1, 2, 3]
        );
        assert_eq!(
            MiniJam::get_system_service_info().unwrap().into_inner(),
            vec![1, 2, 3]
        );
    });
}

#[test]
fn genesis_config_seeds_service_fuel() {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    pallet_minijam::GenesisConfig::<Test> {
        protocol_state: Vec::new(),
        service_fuel: vec![(7, 250), (7, 50), (8, 0)],
        _phantom: Default::default(),
    }
    .assimilate_storage(&mut storage)
    .unwrap();

    sp_io::TestExternalities::from(storage).execute_with(|| {
        let fuel = pallet_minijam::ServiceFuelAccounts::<Test>::get(7);
        assert_eq!(fuel.available, 300);
        assert_eq!(fuel.reserved, 0);
        assert_eq!(pallet_minijam::TotalServiceFuel::<Test>::get(), 300);
        assert_eq!(MiniJam::get_service_fuel(7), fuel);
    });
}

#[test]
fn accepted_candidate_releases_bonds_and_enters_execution_queue() {
    new_test_ext().execute_with(|| {
        let pairs = activate_workers();
        assert_ok!(submit_work(5));
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
        assert_ok!(submit_work(5));
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
            MiniJam::get_protocol_state(key).unwrap().into_inner(),
            vec![1]
        );
        assert_eq!(
            MiniJam::work(0).unwrap().status,
            pallet_minijam::WorkStatus::Imported
        );
        assert!(pallet_minijam::ExecutionQueue::<Test>::get().is_empty());
        assert!(pallet_minijam::ExecutionReceipts::<Test>::get(0).is_some());
        assert!(pallet_minijam::LastExecutionReceipt::<Test>::get().is_some());
        assert_eq!(
            MiniJam::get_last_execution_receipt(),
            pallet_minijam::LastExecutionReceipt::<Test>::get()
        );
    });
}

#[test]
fn queued_preimages_are_imported_with_next_virtual_block() {
    new_test_ext().execute_with(|| {
        let preimage = minijam_protocol::CanonicalPreimageBytes::try_from(vec![7, 8, 9]).unwrap();
        let canonical_hash = blake2_256(&preimage);
        assert_ok!(MiniJam::submit_preimage(RuntimeOrigin::signed(6), preimage));
        assert_eq!(pallet_minijam::PendingPreimages::<Test>::get().len(), 1);
        assert_eq!(MiniJam::get_pending_preimages().len(), 1);
        assert!(MiniJam::has_pending_preimage(7, canonical_hash, 3));

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
        assert!(!MiniJam::has_pending_preimage(7, canonical_hash, 3));
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
        assert_eq!(MiniJam::get_pending_system_ops().len(), 1);
        assert_eq!(
            MiniJam::get_system_op(request_id),
            Some(pending[0].op.clone())
        );

        System::set_block_number(100);
        <MiniJam as frame_support::traits::Hooks<u64>>::on_finalize(100);

        assert!(pallet_minijam::PendingSystemOps::<Test>::get().is_empty());
        assert!(!pallet_minijam::PendingSystemOpKeys::<Test>::contains_key(
            request_id
        ));
        assert!(MiniJam::get_system_op(request_id).is_none());
        let receipt = StateValue::try_from(vec![1, 2, 3]).unwrap();
        pallet_minijam::ProtocolState::<Test>::insert(system_receipt_key(&request_id), &receipt);
        assert_eq!(MiniJam::get_system_receipt(request_id), Some(receipt));
    });
}

#[test]
fn fund_service_moves_balance_to_fuel_escrow() {
    new_test_ext().execute_with(|| {
        pallet_minijam::ProtocolState::<Test>::insert(
            service_info_key(7),
            StateValue::try_from(vec![1, 2, 3]).unwrap(),
        );

        assert_ok!(MiniJam::fund_service(RuntimeOrigin::signed(5), 7, 250));

        let account = pallet_minijam::ServiceFuelAccounts::<Test>::get(7);
        assert_eq!(account.available, 250);
        assert_eq!(account.reserved, 0);
        assert_eq!(pallet_minijam::TotalServiceFuel::<Test>::get(), 250);
        assert_eq!(Balances::total_balance(&5), 10_001 - 250);
        assert_eq!(Balances::total_balance(&101), 250);
    });
}

#[test]
fn fund_service_rejects_unknown_service_and_zero_amount() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            MiniJam::fund_service(RuntimeOrigin::signed(5), 7, 250),
            pallet_minijam::Error::<Test>::UnknownService
        );
        pallet_minijam::ProtocolState::<Test>::insert(
            service_info_key(7),
            StateValue::try_from(vec![1, 2, 3]).unwrap(),
        );
        assert_noop!(
            MiniJam::fund_service(RuntimeOrigin::signed(5), 7, 0),
            pallet_minijam::Error::<Test>::ZeroFuelAmount
        );
    });
}

#[test]
fn submit_work_reserves_service_fuel_by_work_items() {
    new_test_ext().execute_with(|| {
        pallet_minijam::ProtocolState::<Test>::insert(
            service_info_key(7),
            StateValue::try_from(vec![1, 2, 3]).unwrap(),
        );
        assert_ok!(MiniJam::fund_service(RuntimeOrigin::signed(5), 7, 100));

        let package = encoded_work_package(21, vec![work_item(7, 10, 20)]);
        assert_ok!(MiniJam::submit_work(
            RuntimeOrigin::signed(5),
            package,
            bundle_ref(21)
        ));

        let fuel = pallet_minijam::ServiceFuelAccounts::<Test>::get(7);
        assert_eq!(fuel.available, 70);
        assert_eq!(fuel.reserved, 30);
        let work = MiniJam::work(0).unwrap();
        assert_eq!(work.fuel_reservation.len(), 1);
        assert_eq!(work.fuel_reservation[0].service_id, 7);
        assert_eq!(work.fuel_reservation[0].refine_limit, 10);
        assert_eq!(work.fuel_reservation[0].accumulate_limit, 20);
        assert_eq!(work.fuel_reservation[0].reserved, 30);
    });
}

#[test]
fn submit_work_rejects_when_service_fuel_is_insufficient() {
    new_test_ext().execute_with(|| {
        pallet_minijam::ProtocolState::<Test>::insert(
            service_info_key(7),
            StateValue::try_from(vec![1, 2, 3]).unwrap(),
        );
        assert_ok!(MiniJam::fund_service(RuntimeOrigin::signed(5), 7, 10));

        assert_noop!(
            MiniJam::submit_work(
                RuntimeOrigin::signed(5),
                encoded_work_package(22, vec![work_item(7, 10, 20)]),
                bundle_ref(22)
            ),
            pallet_minijam::Error::<Test>::InsufficientServiceFuel
        );
        let fuel = pallet_minijam::ServiceFuelAccounts::<Test>::get(7);
        assert_eq!(fuel.available, 10);
        assert_eq!(fuel.reserved, 0);
    });
}

#[test]
fn failed_work_releases_reserved_service_fuel() {
    new_test_ext().execute_with(|| {
        activate_workers();
        pallet_minijam::ProtocolState::<Test>::insert(
            service_info_key(7),
            StateValue::try_from(vec![1, 2, 3]).unwrap(),
        );
        assert_ok!(MiniJam::fund_service(RuntimeOrigin::signed(5), 7, 100));

        assert_ok!(MiniJam::submit_work(
            RuntimeOrigin::signed(5),
            encoded_work_package(23, vec![work_item(7, 10, 20)]),
            bundle_ref(23)
        ));
        assert_eq!(
            pallet_minijam::ServiceFuelAccounts::<Test>::get(7).reserved,
            30
        );

        for block in [121, 142, 163] {
            System::set_block_number(block);
            <MiniJam as frame_support::traits::Hooks<u64>>::on_initialize(block);
        }

        let fuel = pallet_minijam::ServiceFuelAccounts::<Test>::get(7);
        assert_eq!(fuel.available, 100);
        assert_eq!(fuel.reserved, 0);
        assert_eq!(
            MiniJam::work(0).unwrap().status,
            pallet_minijam::WorkStatus::Failed
        );
        assert!(MiniJam::work(0).unwrap().fuel_reservation.is_empty());
        assert_eq!(
            MiniJam::work_fuel_settlement(0).unwrap(),
            pallet_minijam::WorkFuelSettlement {
                charged: 0,
                refunded: 30
            }
        );
    });
}

#[test]
fn imported_report_settles_reserved_service_fuel() {
    new_test_ext().execute_with(|| {
        let pairs = activate_workers();
        pallet_minijam::ProtocolState::<Test>::insert(
            service_info_key(7),
            StateValue::try_from(vec![1, 2, 3]).unwrap(),
        );
        assert_ok!(MiniJam::fund_service(RuntimeOrigin::signed(5), 7, 100));
        let package = encoded_work_package(24, vec![work_item(7, 10, 20)]);
        let package_hash = blake2_256(&package);
        assert_ok!(MiniJam::submit_work(
            RuntimeOrigin::signed(5),
            package,
            bundle_ref(24)
        ));
        let report = encoded_work_report(package_hash, vec![work_result(7, 4, 6)]);
        let envelope = envelope_with_report(0, 0, report);
        let report_hash = envelope.canonical_report_hash;

        assert_ok!(MiniJam::submit_candidate(
            RuntimeOrigin::signed(6),
            Box::new(envelope)
        ));
        let assignment = Workers::assignment(0, 0).unwrap();
        vote_for_report(
            &pairs[assignment[0] as usize],
            assignment[0],
            report_hash,
            Verdict::Support,
        );
        vote_for_report(
            &pairs[assignment[1] as usize],
            assignment[1],
            report_hash,
            Verdict::Support,
        );
        vote_for_report(
            &pairs[assignment[2] as usize],
            assignment[2],
            report_hash,
            Verdict::Oppose(minijam_protocol::OpposeReason::MissingData),
        );
        <MiniJam as frame_support::traits::Hooks<u64>>::on_initialize(100);

        System::set_block_number(101);
        <MiniJam as frame_support::traits::Hooks<u64>>::on_finalize(101);

        let fuel = pallet_minijam::ServiceFuelAccounts::<Test>::get(7);
        assert_eq!(fuel.available, 90);
        assert_eq!(fuel.reserved, 0);
        assert_eq!(pallet_minijam::TotalServiceFuel::<Test>::get(), 90);
        assert_eq!(Balances::total_balance(&101), 90);
        assert_eq!(MiniJam::get_service_fuel(7), fuel);
        assert_eq!(
            MiniJam::work(0).unwrap().status,
            pallet_minijam::WorkStatus::Imported
        );
        assert!(MiniJam::work(0).unwrap().fuel_reservation.is_empty());
        assert_eq!(
            MiniJam::work_fuel_settlement(0).unwrap(),
            pallet_minijam::WorkFuelSettlement {
                charged: 10,
                refunded: 20
            }
        );
        assert_eq!(
            MiniJam::get_work_fuel_reservation(0).unwrap(),
            MiniJam::work(0).unwrap().fuel_reservation
        );
        assert_eq!(
            MiniJam::get_work_fuel_settlement(0),
            MiniJam::work_fuel_settlement(0)
        );
        assert_eq!(MiniJam::get_candidate(0, 0).unwrap().envelope.work_id, 0);
        assert_eq!(
            MiniJam::get_execution_receipt(0),
            pallet_minijam::ExecutionReceipts::<Test>::get(0)
        );
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
fn system_op_execution_failure_is_quarantined_without_panic() {
    new_test_ext().execute_with(|| {
        assert_ok!(MiniJam::submit_system_op(
            RuntimeOrigin::signed(5),
            Box::new(SystemCommandV1::CreateService {
                code_hash: [0xee; 32],
                code_len: 32,
                min_item_gas: 1,
                min_memo_gas: 1,
                initial_balance: 100,
            })
        ));
        let pending = pallet_minijam::PendingSystemOps::<Test>::get();
        let request_id = pending[0].op.request_id;

        System::set_block_number(100);
        <MiniJam as frame_support::traits::Hooks<u64>>::on_finalize(100);

        assert!(pallet_minijam::PendingSystemOps::<Test>::get().is_empty());
        assert!(!pallet_minijam::PendingSystemOpKeys::<Test>::contains_key(
            request_id
        ));
        let quarantined = pallet_minijam::QuarantinedSystemOps::<Test>::get();
        assert_eq!(quarantined.len(), 1);
        assert_eq!(quarantined[0].op.request_id, request_id);
        assert_eq!(
            quarantined[0].canonical_hash,
            blake2_256(&quarantined[0].op.encode())
        );
        assert_eq!(
            quarantined[0].error_code,
            pallet_minijam::ExecutionErrorCode::ServiceFailure
        );
        assert_eq!(quarantined[0].block_number, 100);
        assert!(quarantined[0].retryable);
        assert!(pallet_minijam::LastExecutionReceipt::<Test>::get().is_none());

        <MiniJam as frame_support::traits::Hooks<u64>>::on_finalize(101);
        assert_eq!(pallet_minijam::QuarantinedSystemOps::<Test>::get().len(), 1);
    });
}

#[test]
fn root_manages_quarantined_system_ops() {
    new_test_ext().execute_with(|| {
        assert_ok!(MiniJam::submit_system_op(
            RuntimeOrigin::signed(5),
            Box::new(SystemCommandV1::CreateService {
                code_hash: [0xee; 32],
                code_len: 32,
                min_item_gas: 1,
                min_memo_gas: 1,
                initial_balance: 100,
            })
        ));
        let first_request_id = pallet_minijam::PendingSystemOps::<Test>::get()[0]
            .op
            .request_id;

        System::set_block_number(100);
        <MiniJam as frame_support::traits::Hooks<u64>>::on_finalize(100);
        assert_eq!(MiniJam::get_quarantined_system_ops().len(), 1);

        assert_ok!(MiniJam::retry_quarantined_system_op(
            RuntimeOrigin::root(),
            first_request_id
        ));
        assert!(MiniJam::get_quarantined_system_ops().is_empty());
        assert_eq!(MiniJam::get_pending_system_ops().len(), 1);
        assert!(pallet_minijam::PendingSystemOpKeys::<Test>::contains_key(
            first_request_id
        ));
        assert_noop!(
            MiniJam::drop_quarantined_system_op(RuntimeOrigin::root(), first_request_id),
            pallet_minijam::Error::<Test>::QuarantinedSystemOpNotFound
        );

        <MiniJam as frame_support::traits::Hooks<u64>>::on_finalize(101);
        assert_eq!(MiniJam::get_quarantined_system_ops().len(), 1);
        assert_ok!(MiniJam::drop_quarantined_system_op(
            RuntimeOrigin::root(),
            first_request_id
        ));
        assert!(MiniJam::get_quarantined_system_ops().is_empty());

        assert_ok!(MiniJam::submit_system_op(
            RuntimeOrigin::signed(5),
            Box::new(SystemCommandV1::CreateService {
                code_hash: [0xee; 32],
                code_len: 32,
                min_item_gas: 1,
                min_memo_gas: 1,
                initial_balance: 100,
            })
        ));
        <MiniJam as frame_support::traits::Hooks<u64>>::on_finalize(102);
        assert_eq!(MiniJam::get_quarantined_system_ops().len(), 1);
        assert_ok!(MiniJam::clear_quarantined_system_ops(RuntimeOrigin::root()));
        assert!(MiniJam::get_quarantined_system_ops().is_empty());
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
        assert_ok!(submit_work(5));
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
        assert_ok!(submit_work(5));
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
        assert_ok!(submit_work(5));
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
        assert_ok!(submit_work(5));
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
        assert_ok!(submit_work(5));
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
        assert_ok!(submit_work(5));
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
