// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use bounded_collections::BoundedVec;
use cid::Cid;
use jam_codec::Encode;
use jambda_minijam_spec::MiniJamSpec;
use jambda_refine::{ImportProofBundle, MiniJamWorkBundleV1, WorkReportInput};
use jp_core_primitives::{
    crypto::OpaqueHash,
    simple::{ByteSequence, TimeSlot},
    spec::ChainSpec,
    traits::JamHash,
    work::{ExtrinsicSpec, RefineContext, WorkItem, WorkPackage},
};
use minijam_protocol::{blake2_256, stage0, ContentRef};
use multihash::Multihash;
use thiserror::Error;

const RAW_CODEC: u64 = 0x55;
const BLAKE2B_256_MULTIHASH: u64 = 0xb220;
const MAX_BATCH_ITEMS: usize = 4;
const ITEM_REFINE_GAS_LIMIT: u64 = MiniJamSpec::MAX_REFINE_GAS / MAX_BATCH_ITEMS as u64;
const ITEM_ACCUMULATE_GAS_LIMIT: u64 = MiniJamSpec::MAX_BLOCK_GAS / MAX_BATCH_ITEMS as u64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildTransactionInput {
    pub payload: Vec<u8>,
    pub extrinsics: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildWorkBatchInput {
    pub service_id: u32,
    pub service_code_hash: [u8; 32],
    pub transactions: Vec<BuildTransactionInput>,
    pub anchor_hash: [u8; 32],
    pub state_root: [u8; 32],
    pub lookup_anchor_slot: u32,
}

#[derive(Clone, Debug)]
pub struct BuiltWorkPackage {
    pub work_package: WorkPackage,
    pub canonical_work_package: Vec<u8>,
    pub bundle: MiniJamWorkBundleV1,
    pub bundle_bytes: Vec<u8>,
    pub package_hash: [u8; 32],
    pub content_ref: ContentRef,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum BuildError {
    #[error("a WorkPackage must contain at least one transaction")]
    EmptyBatch,
    #[error("a WorkPackage exceeds the direct-ingress batch item limit")]
    TooManyItems,
    #[error("extrinsic length exceeds u32")]
    ExtrinsicTooLarge,
    #[error("work package length exceeds the protocol limit")]
    WorkPackageTooLarge,
    #[error("bundle length exceeds u64")]
    BundleTooLarge,
    #[error("invalid CID multihash: {0}")]
    InvalidMultihash(String),
    #[error("CID exceeds the ContentRef limit")]
    CidTooLarge,
}

pub fn build_work_batch(input: BuildWorkBatchInput) -> Result<BuiltWorkPackage, BuildError> {
    if input.transactions.is_empty() {
        return Err(BuildError::EmptyBatch);
    }
    if input.transactions.len() > MAX_BATCH_ITEMS {
        return Err(BuildError::TooManyItems);
    }

    let mut items = Vec::with_capacity(input.transactions.len());
    let mut external_data = Vec::with_capacity(input.transactions.len());
    for transaction in input.transactions {
        let mut extrinsic_specs = Vec::with_capacity(transaction.extrinsics.len());
        let mut item_external_data = Vec::with_capacity(transaction.extrinsics.len());
        for bytes in transaction.extrinsics {
            let len = u32::try_from(bytes.len()).map_err(|_| BuildError::ExtrinsicTooLarge)?;
            extrinsic_specs.push(ExtrinsicSpec {
                hash: OpaqueHash(blake2_256(&bytes)),
                len,
            });
            item_external_data.push(ByteSequence::from(bytes));
        }
        items.push(WorkItem {
            service: input.service_id,
            code_hash: OpaqueHash(input.service_code_hash),
            refine_gas_limit: ITEM_REFINE_GAS_LIMIT,
            accumulate_gas_limit: ITEM_ACCUMULATE_GAS_LIMIT,
            export_count: 0,
            payload: ByteSequence::from(transaction.payload),
            import_segments: Vec::new(),
            extrinsic: extrinsic_specs,
        });
        external_data.push(item_external_data);
    }

    let work_package = WorkPackage {
        auth_code_host: stage0::AUTH_CODE_HOST,
        auth_code_hash: OpaqueHash(stage0::AUTH_CODE_HASH),
        context: RefineContext {
            anchor: OpaqueHash(input.anchor_hash),
            state_root: OpaqueHash(input.state_root),
            beefy_root: OpaqueHash([0; 32]),
            lookup_anchor: OpaqueHash(input.anchor_hash),
            lookup_anchor_slot: TimeSlot(input.lookup_anchor_slot),
            prerequisites: Vec::new(),
        },
        authorization: ByteSequence::from(Vec::new()),
        authorizer_config: ByteSequence::from(Vec::new()),
        items,
    };
    let canonical_work_package = work_package.encode();
    if canonical_work_package.len() > 1_048_576 {
        return Err(BuildError::WorkPackageTooLarge);
    }
    let package_hash = work_package.jam_hash().0;
    let report_input = WorkReportInput {
        core_index: stage0::CORE_INDEX,
        work_package: Arc::new(work_package.clone()),
        external_data: Arc::new(external_data),
        import_segments: Arc::new(vec![Vec::new(); work_package.items.len()]),
        import_proofs: ImportProofBundle::default(),
    };
    let bundle = MiniJamWorkBundleV1::new(&report_input);
    let bundle_bytes = report_input.encode_auditable_bundle();
    let content_ref = content_ref(&bundle_bytes)?;

    Ok(BuiltWorkPackage {
        work_package,
        canonical_work_package,
        bundle,
        bundle_bytes,
        package_hash,
        content_ref,
    })
}

/// Build the v1 single-transaction shape using the same canonical batch
/// builder. This is useful for callers migrating incrementally; it does not
/// create a second execution path.
pub fn build_work_package(input: BuildWorkInput) -> Result<BuiltWorkPackage, BuildError> {
    build_work_batch(BuildWorkBatchInput {
        service_id: input.service_id,
        service_code_hash: input.service_code_hash,
        transactions: vec![BuildTransactionInput {
            payload: input.payload,
            extrinsics: input.extrinsics,
        }],
        anchor_hash: input.anchor_hash,
        state_root: input.state_root,
        lookup_anchor_slot: input.lookup_anchor_slot,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildWorkInput {
    pub service_id: u32,
    pub service_code_hash: [u8; 32],
    pub payload: Vec<u8>,
    pub extrinsics: Vec<Vec<u8>>,
    pub anchor_hash: [u8; 32],
    pub state_root: [u8; 32],
    pub lookup_anchor_slot: u32,
}

fn content_ref(bytes: &[u8]) -> Result<ContentRef, BuildError> {
    let content_hash = blake2_256(bytes);
    let multihash = Multihash::<64>::wrap(BLAKE2B_256_MULTIHASH, &content_hash)
        .map_err(|error| BuildError::InvalidMultihash(error.to_string()))?;
    let cid = Cid::new_v1(RAW_CODEC, multihash).to_bytes();
    let cid_v1 = BoundedVec::try_from(cid).map_err(|_| BuildError::CidTooLarge)?;
    let size = u64::try_from(bytes.len()).map_err(|_| BuildError::BundleTooLarge)?;
    Ok(ContentRef {
        cid_v1,
        content_hash,
        size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jam_codec::Decode;

    fn fixture() -> BuildWorkInput {
        BuildWorkInput {
            service_id: 42,
            service_code_hash: [0x11; 32],
            payload: b"increment".to_vec(),
            extrinsics: vec![b"first".to_vec(), b"second".to_vec()],
            anchor_hash: [0x22; 32],
            state_root: [0x33; 32],
            lookup_anchor_slot: 9,
        }
    }

    #[test]
    fn same_input_builds_identical_package_and_bundle() {
        let first = build_work_package(fixture()).unwrap();
        let second = build_work_package(fixture()).unwrap();

        assert_eq!(first.canonical_work_package, second.canonical_work_package);
        assert_eq!(first.bundle_bytes, second.bundle_bytes);
        assert_eq!(first.package_hash, second.package_hash);
        assert_eq!(first.content_ref, second.content_ref);
    }

    #[test]
    fn bundle_contains_fixed_stage0_shape_and_ordered_extrinsics() {
        let built = build_work_package(fixture()).unwrap();
        let mut encoded = built.bundle_bytes.as_slice();
        let decoded = MiniJamWorkBundleV1::decode(&mut encoded).unwrap();

        assert!(encoded.is_empty());
        assert!(decoded.package_hash_matches());
        assert_eq!(decoded.package_hash.0, built.package_hash);
        assert_eq!(decoded.work_package.auth_code_host, stage0::AUTH_CODE_HOST);
        assert_eq!(
            decoded.work_package.auth_code_hash.0,
            stage0::AUTH_CODE_HASH
        );
        assert_eq!(decoded.work_package.context.anchor.0, [0x22; 32]);
        assert_eq!(decoded.work_package.context.lookup_anchor.0, [0x22; 32]);
        assert_eq!(decoded.work_package.context.state_root.0, [0x33; 32]);
        assert_eq!(decoded.work_package.context.lookup_anchor_slot.0, 9);
        assert!(decoded.work_package.context.prerequisites.is_empty());
        assert_eq!(decoded.work_package.items.len(), 1);
        assert!(decoded.work_package.items[0].import_segments.is_empty());
        assert_eq!(decoded.work_package.items[0].export_count, 0);
        assert_eq!(
            decoded.external_data,
            vec![vec![
                ByteSequence::from(b"first".to_vec()),
                ByteSequence::from(b"second".to_vec())
            ]]
        );
    }

    #[test]
    fn bundle_and_cid_match_golden_values() {
        let built = build_work_package(fixture()).unwrap();

        assert_eq!(
            built.package_hash,
            [
                46, 143, 60, 25, 243, 178, 252, 66, 19, 162, 189, 154, 230, 220, 54, 92, 253, 89,
                13, 28, 5, 207, 158, 200, 209, 165, 164, 20, 29, 226, 141, 59,
            ]
        );
        assert_eq!(
            built.content_ref.content_hash,
            [
                206, 177, 76, 220, 180, 2, 33, 227, 83, 40, 185, 155, 181, 32, 182, 166, 252, 185,
                35, 100, 42, 186, 215, 140, 187, 93, 224, 197, 167, 177, 7, 228,
            ]
        );
        assert_eq!(
            built.content_ref.cid_v1.as_slice(),
            &[
                1, 85, 160, 228, 2, 32, 206, 177, 76, 220, 180, 2, 33, 227, 83, 40, 185, 155, 181,
                32, 182, 166, 252, 185, 35, 100, 42, 186, 215, 140, 187, 93, 224, 197, 167, 177, 7,
                228,
            ]
        );
    }

    #[test]
    fn batch_builder_keeps_one_item_per_transaction_and_shared_context() {
        let built = build_work_batch(BuildWorkBatchInput {
            service_id: 42,
            service_code_hash: [0x11; 32],
            transactions: vec![
                BuildTransactionInput {
                    payload: b"first".to_vec(),
                    extrinsics: vec![],
                },
                BuildTransactionInput {
                    payload: b"second".to_vec(),
                    extrinsics: vec![b"x".to_vec()],
                },
            ],
            anchor_hash: [0x22; 32],
            state_root: [0x33; 32],
            lookup_anchor_slot: 9,
        })
        .unwrap();
        assert_eq!(built.work_package.items.len(), 2);
        assert_eq!(built.work_package.items[0].payload.as_slice(), b"first");
        assert_eq!(built.work_package.items[1].payload.as_slice(), b"second");
        assert_eq!(built.bundle.external_data.len(), 2);
        assert_eq!(built.bundle.work_package.context.anchor.0, [0x22; 32]);
        assert_eq!(
            built.bundle.work_package.context.lookup_anchor.0,
            [0x22; 32]
        );
        assert_eq!(built.bundle.work_package.context.lookup_anchor_slot.0, 9);
    }

    #[test]
    fn batch_builder_rejects_empty_input() {
        let result = build_work_batch(BuildWorkBatchInput {
            service_id: 1,
            service_code_hash: [0; 32],
            transactions: vec![],
            anchor_hash: [0; 32],
            state_root: [0; 32],
            lookup_anchor_slot: 0,
        });
        assert!(matches!(result, Err(BuildError::EmptyBatch)));
    }

    #[test]
    fn counter_service_payload_builds_auditable_bundle() {
        let blob = include_bytes!("../../../examples/services/counter/artifacts/counter-c.blob");
        let built = build_work_package(BuildWorkInput {
            service_id: 10,
            service_code_hash: blake2_256(blob),
            payload: 1_i64.to_le_bytes().to_vec(),
            extrinsics: Vec::new(),
            anchor_hash: [0x44; 32],
            state_root: [0x55; 32],
            lookup_anchor_slot: 12,
        })
        .unwrap();
        let mut encoded = built.bundle_bytes.as_slice();
        let decoded = MiniJamWorkBundleV1::decode(&mut encoded).unwrap();

        assert!(encoded.is_empty());
        assert!(decoded.package_hash_matches());
        assert_eq!(decoded.work_package.items[0].service, 10);
        assert_eq!(
            decoded.work_package.items[0].payload.as_slice(),
            1_i64.to_le_bytes()
        );
        assert_eq!(decoded.work_package.items[0].code_hash.0, blake2_256(blob));
    }
}
