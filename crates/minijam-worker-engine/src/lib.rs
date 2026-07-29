// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{collections::BTreeMap, vec::Vec};
use bounded_collections::{BoundedVec, ConstU32};
use minijam_protocol::{
    blake2_256, AssignmentRound, BlockNumber, ContentRef, Hash, Verdict, WorkId, WorkerId,
    MINIMUM_ABSENCE_SLASH, MINIMUM_WORKER_STAKE, OPPOSE_THRESHOLD, SUPPORT_THRESHOLD,
    TIMELY_VOTE_REWARD, WORKERS_PER_WORK,
};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

pub type AssignedWorkers = BoundedVec<WorkerId, ConstU32<3>>;
pub type VoteRecords = BoundedVec<VoteRecord, ConstU32<3>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContentVerificationError {
    EmptyCid,
    SizeLimitExceeded,
    SizeMismatch,
    HashMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkBundleDecodeError {
    InvalidEncoding,
    TrailingBytes,
    UnsupportedVersion,
    PackageHashMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkBundleVerificationError {
    Content(ContentVerificationError),
    Decode(WorkBundleDecodeError),
    PackageHashMismatch,
}

pub trait WorkBundleDecoder {
    fn package_hash(&self, bytes: &[u8]) -> Result<Hash, WorkBundleDecodeError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MiniJamWorkBundleDecoder;

impl MiniJamWorkBundleDecoder {
    #[cfg(feature = "std")]
    pub fn decode(
        &self,
        bytes: &[u8],
    ) -> Result<jambda_refine::MiniJamWorkBundleV1, WorkBundleDecodeError> {
        use jam_codec::Decode;

        let mut input = bytes;
        let bundle = jambda_refine::MiniJamWorkBundleV1::decode(&mut input)
            .map_err(|_| WorkBundleDecodeError::InvalidEncoding)?;
        if !input.is_empty() {
            return Err(WorkBundleDecodeError::TrailingBytes);
        }
        if bundle.version != jambda_refine::MINIJAM_WORK_BUNDLE_VERSION_V1 {
            return Err(WorkBundleDecodeError::UnsupportedVersion);
        }
        if !bundle.package_hash_matches() {
            return Err(WorkBundleDecodeError::PackageHashMismatch);
        }
        Ok(bundle)
    }
}

impl WorkBundleDecoder for MiniJamWorkBundleDecoder {
    fn package_hash(&self, bytes: &[u8]) -> Result<Hash, WorkBundleDecodeError> {
        #[cfg(not(feature = "std"))]
        {
            let _ = bytes;
            return Err(WorkBundleDecodeError::UnsupportedVersion);
        }

        #[cfg(feature = "std")]
        Ok(self.decode(bytes)?.package_hash.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedWorkBundle<'a> {
    pub bytes: &'a [u8],
    pub package_hash: Hash,
}

pub fn verify_content_ref(
    reference: &ContentRef,
    bytes: &[u8],
    max_bytes: u64,
) -> Result<(), ContentVerificationError> {
    if reference.cid_v1.is_empty() {
        return Err(ContentVerificationError::EmptyCid);
    }
    if reference.size > max_bytes {
        return Err(ContentVerificationError::SizeLimitExceeded);
    }
    if bytes.len() as u64 != reference.size {
        return Err(ContentVerificationError::SizeMismatch);
    }
    if blake2_256(bytes) != reference.content_hash {
        return Err(ContentVerificationError::HashMismatch);
    }
    Ok(())
}

pub fn verify_work_bundle<'a, D: WorkBundleDecoder>(
    reference: &ContentRef,
    bytes: &'a [u8],
    max_bytes: u64,
    expected_package_hash: Hash,
    decoder: &D,
) -> Result<VerifiedWorkBundle<'a>, WorkBundleVerificationError> {
    verify_content_ref(reference, bytes, max_bytes)
        .map_err(WorkBundleVerificationError::Content)?;
    let package_hash = decoder
        .package_hash(bytes)
        .map_err(WorkBundleVerificationError::Decode)?;
    if package_hash != expected_package_hash {
        return Err(WorkBundleVerificationError::PackageHashMismatch);
    }
    Ok(VerifiedWorkBundle {
        bytes,
        package_hash,
    })
}

#[cfg(feature = "std")]
pub mod fetch {
    use super::{verify_content_ref, ContentRef, ContentVerificationError};
    use alloc::{collections::BTreeMap, format, string::String, vec::Vec};

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum FetchError {
        InvalidReference,
        NotFound,
        Transport(String),
        Verification(ContentVerificationError),
    }

    #[async_trait::async_trait]
    pub trait ContentFetcher: Send + Sync {
        async fn fetch(&self, reference: &ContentRef) -> Result<Vec<u8>, FetchError>;
    }

    #[async_trait::async_trait]
    pub trait HttpBytesClient: Send + Sync {
        async fn get_bytes(&self, url: &str) -> Result<Vec<u8>, FetchError>;
    }

    pub async fn fetch_verified_content<F: ContentFetcher + ?Sized>(
        fetcher: &F,
        reference: &ContentRef,
        max_bytes: u64,
    ) -> Result<Vec<u8>, FetchError> {
        let bytes = fetcher.fetch(reference).await?;
        verify_content_ref(reference, &bytes, max_bytes).map_err(FetchError::Verification)?;
        Ok(bytes)
    }

    #[derive(Clone, Debug)]
    pub struct HttpContentFetcher<C> {
        client: C,
    }

    impl<C> HttpContentFetcher<C> {
        pub fn new(client: C) -> Self {
            Self { client }
        }
    }

    #[async_trait::async_trait]
    impl<C: HttpBytesClient> ContentFetcher for HttpContentFetcher<C> {
        async fn fetch(&self, reference: &ContentRef) -> Result<Vec<u8>, FetchError> {
            let url = core::str::from_utf8(reference.cid_v1.as_slice())
                .map_err(|_| FetchError::InvalidReference)?;
            self.client.get_bytes(url).await
        }
    }

    #[derive(Clone, Debug)]
    pub struct IpfsGatewayFetcher<C> {
        client: C,
        gateway: String,
    }

    impl<C> IpfsGatewayFetcher<C> {
        pub fn new(client: C, gateway: impl Into<String>) -> Self {
            Self {
                client,
                gateway: gateway.into().trim_end_matches('/').into(),
            }
        }
    }

    #[async_trait::async_trait]
    impl<C: HttpBytesClient> ContentFetcher for IpfsGatewayFetcher<C> {
        async fn fetch(&self, reference: &ContentRef) -> Result<Vec<u8>, FetchError> {
            let cid = cid::Cid::try_from(reference.cid_v1.as_slice())
                .map_err(|_| FetchError::InvalidReference)?;
            self.client
                .get_bytes(&format!("{}/ipfs/{}", self.gateway, cid))
                .await
        }
    }

    #[derive(Clone, Debug, Default)]
    pub struct MemoryContentFetcher {
        entries: BTreeMap<Vec<u8>, Vec<u8>>,
    }

    impl MemoryContentFetcher {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn insert(&mut self, reference: &ContentRef, bytes: Vec<u8>) {
            self.entries.insert(reference.cid_v1.to_vec(), bytes);
        }

        pub fn with_content(mut self, reference: &ContentRef, bytes: Vec<u8>) -> Self {
            self.insert(reference, bytes);
            self
        }
    }

    #[async_trait::async_trait]
    impl ContentFetcher for MemoryContentFetcher {
        async fn fetch(&self, reference: &ContentRef) -> Result<Vec<u8>, FetchError> {
            self.entries
                .get(reference.cid_v1.as_slice())
                .cloned()
                .ok_or(FetchError::NotFound)
        }
    }
}

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum WorkerStatus {
    Active,
    SuspendedUntil(u32),
    UnbondingUntil(u32),
    Exited,
}

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct WorkerRecord {
    pub id: WorkerId,
    pub active_stake: u128,
    pub session_key: [u8; 32],
    pub status: WorkerStatus,
}

impl WorkerRecord {
    pub fn eligible_at(&self, epoch: u32) -> bool {
        self.active_stake >= MINIMUM_WORKER_STAKE
            && match self.status {
                WorkerStatus::Active => true,
                WorkerStatus::SuspendedUntil(until) => epoch >= until,
                WorkerStatus::UnbondingUntil(_) | WorkerStatus::Exited => false,
            }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssignmentError {
    InsufficientWorkers,
    CapacityExceeded,
}

pub fn top_workers(
    workers: impl IntoIterator<Item = WorkerRecord>,
    epoch: u32,
    top_n: usize,
) -> Vec<WorkerRecord> {
    let mut eligible: Vec<_> = workers
        .into_iter()
        .filter(|worker| worker.eligible_at(epoch))
        .collect();
    eligible.sort_by(|a, b| {
        b.active_stake
            .cmp(&a.active_stake)
            .then_with(|| a.id.cmp(&b.id))
    });
    eligible.truncate(top_n);
    eligible
}

pub fn assign_batch(
    seed: Hash,
    works: &[(WorkId, AssignmentRound)],
    pool: &[WorkerRecord],
    max_duties: u32,
) -> Result<BTreeMap<WorkId, AssignedWorkers>, AssignmentError> {
    if pool.len() < WORKERS_PER_WORK as usize {
        return Err(AssignmentError::InsufficientWorkers);
    }

    let mut duties = BTreeMap::<WorkerId, u32>::new();
    let mut assignments = BTreeMap::new();
    for (work_id, round) in works {
        let mut candidates: Vec<_> = pool
            .iter()
            .filter(|worker| duties.get(&worker.id).copied().unwrap_or(0) < max_duties)
            .map(|worker| {
                let mut bytes = b"minijam/assignment-v1".to_vec();
                bytes.extend(seed);
                bytes.extend(work_id.to_le_bytes());
                bytes.push(*round);
                bytes.extend(worker.id.to_le_bytes());
                (blake2_256(&bytes), worker.id)
            })
            .collect();
        candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        if candidates.len() < WORKERS_PER_WORK as usize {
            return Err(AssignmentError::CapacityExceeded);
        }
        let selected = candidates
            .into_iter()
            .take(WORKERS_PER_WORK as usize)
            .map(|(_, worker_id)| {
                duties
                    .entry(worker_id)
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
                worker_id
            })
            .collect::<Vec<_>>();
        assignments.insert(
            *work_id,
            BoundedVec::try_from(selected).map_err(|_| AssignmentError::CapacityExceeded)?,
        );
    }
    Ok(assignments)
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct VoteRecord {
    pub worker_id: WorkerId,
    pub verdict: Verdict,
    pub submitted_at: BlockNumber,
}

#[derive(Clone, Copy, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub enum LockedOutcome {
    Accepted,
    Rejected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VoteError {
    NotAssigned,
    DuplicateVote,
    Closed,
    BeforeCandidate,
    BoundExceeded,
}

#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct CandidateRound {
    pub work_id: WorkId,
    pub round: AssignmentRound,
    pub assigned: AssignedWorkers,
    pub report_hash: Option<Hash>,
    pub report_deadline: BlockNumber,
    pub vote_deadline: Option<BlockNumber>,
    pub votes: VoteRecords,
    pub locked_outcome: Option<LockedOutcome>,
}

impl CandidateRound {
    pub fn new(
        work_id: WorkId,
        round: AssignmentRound,
        assigned: AssignedWorkers,
        report_deadline: BlockNumber,
    ) -> Self {
        Self {
            work_id,
            round,
            assigned,
            report_hash: None,
            report_deadline,
            vote_deadline: None,
            votes: Default::default(),
            locked_outcome: None,
        }
    }

    pub fn open_candidate(
        &mut self,
        report_hash: Hash,
        now: BlockNumber,
        vote_window: BlockNumber,
    ) -> Result<(), VoteError> {
        if self.report_hash.is_some() || now > self.report_deadline {
            return Err(VoteError::Closed);
        }
        self.report_hash = Some(report_hash);
        self.vote_deadline = Some(now.saturating_add(vote_window));
        Ok(())
    }

    pub fn record_vote(
        &mut self,
        worker_id: WorkerId,
        verdict: Verdict,
        now: BlockNumber,
    ) -> Result<Option<LockedOutcome>, VoteError> {
        let deadline = self.vote_deadline.ok_or(VoteError::BeforeCandidate)?;
        if now > deadline {
            return Err(VoteError::Closed);
        }
        if !self.assigned.contains(&worker_id) {
            return Err(VoteError::NotAssigned);
        }
        if self.votes.iter().any(|vote| vote.worker_id == worker_id) {
            return Err(VoteError::DuplicateVote);
        }
        self.votes
            .try_push(VoteRecord {
                worker_id,
                verdict,
                submitted_at: now,
            })
            .map_err(|_| VoteError::BoundExceeded)?;
        self.update_locked_outcome();
        Ok(self.locked_outcome)
    }

    pub fn ready_to_finalize(&self, now: BlockNumber) -> bool {
        self.vote_deadline
            .is_some_and(|deadline| now >= deadline || self.votes.len() == self.assigned.len())
    }

    pub fn final_outcome(&self, now: BlockNumber) -> Option<LockedOutcome> {
        self.ready_to_finalize(now)
            .then_some(self.locked_outcome.unwrap_or(LockedOutcome::Rejected))
    }

    pub fn absent_workers(&self, now: BlockNumber) -> Vec<WorkerId> {
        if !self.ready_to_finalize(now) {
            return Vec::new();
        }
        self.assigned
            .iter()
            .copied()
            .filter(|worker| !self.votes.iter().any(|vote| vote.worker_id == *worker))
            .collect()
    }

    fn update_locked_outcome(&mut self) {
        let support = self
            .votes
            .iter()
            .filter(|vote| matches!(vote.verdict, Verdict::Support))
            .count() as u32;
        let oppose = self
            .votes
            .iter()
            .filter(|vote| matches!(vote.verdict, Verdict::Oppose(_)))
            .count() as u32;
        if support >= SUPPORT_THRESHOLD {
            self.locked_outcome = Some(LockedOutcome::Accepted);
        } else if oppose >= OPPOSE_THRESHOLD {
            self.locked_outcome = Some(LockedOutcome::Rejected);
        }
    }
}

pub fn timely_vote_reward() -> u128 {
    TIMELY_VOTE_REWARD
}

pub fn absence_slash(stake: u128) -> u128 {
    stake
        .saturating_div(100)
        .max(MINIMUM_ABSENCE_SLASH)
        .min(stake)
}

pub fn equivocation_slash(stake: u128) -> u128 {
    stake.saturating_mul(20).saturating_div(100)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::{
        fetch_verified_content, FetchError, HttpBytesClient, HttpContentFetcher,
        IpfsGatewayFetcher, MemoryContentFetcher,
    };
    use alloc::sync::Arc;
    use futures::executor::block_on;
    use jam_codec::Encode as JamEncode;
    use jp_core_primitives::{
        crypto::OpaqueHash,
        simple::{ByteSequence, TimeSlot},
        traits::JamHash,
        work::{RefineContext, WorkPackage},
    };
    use minijam_protocol::ContentRef;
    use minijam_protocol::{OpposeReason, UNIT, VOTE_WINDOW};

    #[derive(Clone)]
    struct TestHttpClient {
        responses: BTreeMap<String, Vec<u8>>,
    }

    #[async_trait::async_trait]
    impl HttpBytesClient for TestHttpClient {
        async fn get_bytes(&self, url: &str) -> Result<Vec<u8>, FetchError> {
            self.responses.get(url).cloned().ok_or(FetchError::NotFound)
        }
    }

    fn worker(id: WorkerId, stake: u128) -> WorkerRecord {
        WorkerRecord {
            id,
            active_stake: stake,
            session_key: [id as u8; 32],
            status: WorkerStatus::Active,
        }
    }

    fn content_ref(bytes: &[u8]) -> ContentRef {
        ContentRef {
            cid_v1: vec![1].try_into().unwrap(),
            content_hash: blake2_256(bytes),
            size: bytes.len() as u64,
        }
    }

    fn content_ref_with_location(bytes: &[u8], location: &[u8]) -> ContentRef {
        ContentRef {
            cid_v1: location.to_vec().try_into().unwrap(),
            content_hash: blake2_256(bytes),
            size: bytes.len() as u64,
        }
    }

    fn refine_package() -> WorkPackage {
        WorkPackage {
            auth_code_host: 0,
            auth_code_hash: OpaqueHash([1u8; 32]),
            context: RefineContext {
                anchor: OpaqueHash([2u8; 32]),
                state_root: OpaqueHash([3u8; 32]),
                beefy_root: OpaqueHash([4u8; 32]),
                lookup_anchor: OpaqueHash([5u8; 32]),
                lookup_anchor_slot: TimeSlot(6),
                prerequisites: Vec::new(),
            },
            authorization: ByteSequence::from(Vec::new()),
            authorizer_config: ByteSequence::from(Vec::new()),
            items: Vec::new(),
        }
    }

    fn refine_bundle_bytes() -> (Vec<u8>, Hash) {
        let package = refine_package();
        let package_hash = package.jam_hash().0;
        let input = jambda_refine::WorkReportInput {
            core_index: 0,
            work_package: Arc::new(package),
            external_data: Arc::new(Vec::new()),
            import_segments: Arc::new(Vec::new()),
            import_proofs: Default::default(),
        };
        (
            jambda_refine::MiniJamWorkBundleV1::new(&input).encode(),
            package_hash,
        )
    }

    #[test]
    fn verifies_content_ref_size_and_hash() {
        let bytes = b"bundle";
        let reference = content_ref(bytes);

        assert_eq!(verify_content_ref(&reference, bytes, 32), Ok(()));
        assert_eq!(
            verify_content_ref(&reference, b"bundlx", 32),
            Err(ContentVerificationError::HashMismatch)
        );
        assert_eq!(
            verify_content_ref(&reference, b"bundle-extra", 32),
            Err(ContentVerificationError::SizeMismatch)
        );
        assert_eq!(
            verify_content_ref(&reference, bytes, 1),
            Err(ContentVerificationError::SizeLimitExceeded)
        );
    }

    #[test]
    fn verifies_work_bundle_content_and_package_hash() {
        let (bytes, package_hash) = refine_bundle_bytes();
        let reference = content_ref(&bytes);

        let verified = verify_work_bundle(
            &reference,
            &bytes,
            1024,
            package_hash,
            &MiniJamWorkBundleDecoder,
        )
        .unwrap();

        assert_eq!(verified.bytes, bytes.as_slice());
        assert_eq!(verified.package_hash, package_hash);
        assert_eq!(
            verify_work_bundle(
                &reference,
                &bytes,
                1024,
                [8u8; 32],
                &MiniJamWorkBundleDecoder
            ),
            Err(WorkBundleVerificationError::PackageHashMismatch)
        );
        assert_eq!(
            verify_work_bundle(
                &reference,
                b"bundle-with-wrong-hash",
                1024,
                package_hash,
                &MiniJamWorkBundleDecoder
            ),
            Err(WorkBundleVerificationError::Content(
                ContentVerificationError::SizeMismatch
            ))
        );
    }

    #[test]
    fn rejects_bundle_when_decoder_cannot_read_package_hash() {
        let bytes = b"short";
        let reference = content_ref(bytes);

        assert_eq!(
            verify_work_bundle(&reference, bytes, 64, [7u8; 32], &MiniJamWorkBundleDecoder),
            Err(WorkBundleVerificationError::Decode(
                WorkBundleDecodeError::InvalidEncoding
            ))
        );
    }

    #[test]
    fn real_work_bundle_decoder_rejects_trailing_bytes_and_unknown_versions() {
        let decoder = MiniJamWorkBundleDecoder;
        let (mut bytes, _) = refine_bundle_bytes();
        bytes.push(0);

        assert_eq!(
            decoder.package_hash(&bytes),
            Err(WorkBundleDecodeError::TrailingBytes)
        );

        let package = refine_package();
        let input = jambda_refine::WorkReportInput {
            core_index: 0,
            work_package: Arc::new(package),
            external_data: Arc::new(Vec::new()),
            import_segments: Arc::new(Vec::new()),
            import_proofs: Default::default(),
        };
        let mut bundle = jambda_refine::MiniJamWorkBundleV1::new(&input);
        bundle.version = 999;
        assert_eq!(
            decoder.package_hash(&bundle.encode()),
            Err(WorkBundleDecodeError::UnsupportedVersion)
        );

        let mut mismatched = bundle;
        mismatched.version = jambda_refine::MINIJAM_WORK_BUNDLE_VERSION_V1;
        mismatched.package_hash = OpaqueHash([9u8; 32]);
        assert_eq!(
            decoder.package_hash(&mismatched.encode()),
            Err(WorkBundleDecodeError::PackageHashMismatch)
        );
    }

    #[test]
    fn memory_fetcher_returns_verified_content() {
        let bytes = b"bundle".to_vec();
        let reference = content_ref(&bytes);
        let fetcher = MemoryContentFetcher::new().with_content(&reference, bytes.clone());

        assert_eq!(
            block_on(fetch_verified_content(&fetcher, &reference, 32)).unwrap(),
            bytes
        );
    }

    #[test]
    fn memory_fetcher_reports_missing_and_invalid_content() {
        let bytes = b"bundle".to_vec();
        let reference = content_ref(&bytes);
        let fetcher = MemoryContentFetcher::new();
        assert_eq!(
            block_on(fetch_verified_content(&fetcher, &reference, 32)),
            Err(FetchError::NotFound)
        );

        let fetcher = MemoryContentFetcher::new().with_content(&reference, b"wrong".to_vec());
        assert_eq!(
            block_on(fetch_verified_content(&fetcher, &reference, 32)),
            Err(FetchError::Verification(
                ContentVerificationError::SizeMismatch
            ))
        );
    }

    #[test]
    fn http_fetcher_uses_reference_location_as_url() {
        let bytes = b"bundle".to_vec();
        let reference = content_ref_with_location(&bytes, b"https://example.test/bundle");
        let fetcher = HttpContentFetcher::new(TestHttpClient {
            responses: BTreeMap::from([("https://example.test/bundle".into(), bytes.clone())]),
        });

        assert_eq!(
            block_on(fetch_verified_content(&fetcher, &reference, 32)).unwrap(),
            bytes
        );
    }

    #[test]
    fn ipfs_fetcher_builds_gateway_url_from_cid() {
        let bytes = b"bundle".to_vec();
        let cid: cid::Cid = "bafk2bzacec76aht7e3ngewvy5k4mzhbksmn2dn5536dodvmc4f7arlrlldixy"
            .parse()
            .unwrap();
        let reference = content_ref_with_location(&bytes, &cid.to_bytes());
        let fetcher = IpfsGatewayFetcher::new(
            TestHttpClient {
                responses: BTreeMap::from([(
                    format!("http://127.0.0.1:8080/ipfs/{cid}"),
                    bytes.clone(),
                )]),
            },
            "http://127.0.0.1:8080/ipfs/..",
        );

        assert_eq!(
            block_on(fetch_verified_content(&fetcher, &reference, 32)),
            Err(FetchError::NotFound)
        );

        let fetcher = IpfsGatewayFetcher::new(
            TestHttpClient {
                responses: BTreeMap::from([(
                    format!("http://127.0.0.1:8080/ipfs/{cid}"),
                    bytes.clone(),
                )]),
            },
            "http://127.0.0.1:8080",
        );
        assert_eq!(
            block_on(fetch_verified_content(&fetcher, &reference, 32)).unwrap(),
            bytes
        );
    }

    #[test]
    fn ranking_is_stake_then_id() {
        let selected = top_workers(
            [
                worker(3, 2_000 * UNIT),
                worker(2, 2_000 * UNIT),
                worker(1, UNIT),
            ],
            0,
            2,
        );
        assert_eq!(selected.iter().map(|w| w.id).collect::<Vec<_>>(), [2, 3]);
    }

    #[test]
    fn assignment_is_deterministic_and_distinct() {
        let pool = (0..8)
            .map(|id| worker(id, (10_000 - id as u128) * UNIT))
            .collect::<Vec<_>>();
        let works = [(1, 0), (2, 0), (3, 0), (4, 0)];
        let first = assign_batch([7u8; 32], &works, &pool, 2).unwrap();
        let second = assign_batch([7u8; 32], &works, &pool, 2).unwrap();
        assert_eq!(first, second);
        assert!(first.values().all(|assigned| {
            assigned.len() == 3
                && assigned[0] != assigned[1]
                && assigned[1] != assigned[2]
                && assigned[0] != assigned[2]
        }));
    }

    #[test]
    fn support_locks_but_attendance_waits() {
        let assigned = BoundedVec::try_from(vec![1, 2, 3]).unwrap();
        let mut round = CandidateRound::new(9, 0, assigned, 20);
        round.open_candidate([4u8; 32], 5, VOTE_WINDOW).unwrap();
        assert_eq!(round.record_vote(1, Verdict::Support, 6).unwrap(), None);
        assert_eq!(
            round.record_vote(2, Verdict::Support, 7).unwrap(),
            Some(LockedOutcome::Accepted)
        );
        assert!(!round.ready_to_finalize(7));
        assert_eq!(round.absent_workers(15), vec![3]);
    }

    #[test]
    fn opposition_counts_as_attendance() {
        let assigned = BoundedVec::try_from(vec![1, 2, 3]).unwrap();
        let mut round = CandidateRound::new(9, 0, assigned, 20);
        round.open_candidate([4u8; 32], 5, VOTE_WINDOW).unwrap();
        round
            .record_vote(1, Verdict::Oppose(OpposeReason::InvalidRefine), 6)
            .unwrap();
        round
            .record_vote(2, Verdict::Oppose(OpposeReason::MissingData), 7)
            .unwrap();
        round.record_vote(3, Verdict::Support, 8).unwrap();
        assert_eq!(round.final_outcome(8), Some(LockedOutcome::Rejected));
        assert!(round.absent_workers(8).is_empty());
    }
}
