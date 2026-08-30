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
    #[error("extrinsic length exceeds u32")]
    ExtrinsicTooLarge,
    #[error("bundle length exceeds u64")]
    BundleTooLarge,
    #[error("invalid CID multihash: {0}")]
    InvalidMultihash(String),
    #[error("CID exceeds the ContentRef limit")]
    CidTooLarge,
}

pub fn build_work_package(input: BuildWorkInput) -> Result<BuiltWorkPackage, BuildError> {
    let mut extrinsic_specs = Vec::with_capacity(input.extrinsics.len());
    let mut external_data = Vec::with_capacity(input.extrinsics.len());
    for bytes in input.extrinsics {
        let len = u32::try_from(bytes.len()).map_err(|_| BuildError::ExtrinsicTooLarge)?;
        extrinsic_specs.push(ExtrinsicSpec {
            hash: OpaqueHash(blake2_256(&bytes)),
            len,
        });
        external_data.push(ByteSequence::from(bytes));
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
        items: vec![WorkItem {
            service: input.service_id,
            code_hash: OpaqueHash(input.service_code_hash),
            refine_gas_limit: MiniJamSpec::MAX_REFINE_GAS,
            accumulate_gas_limit: MiniJamSpec::MAX_REFINE_GAS,
            export_count: 0,
            payload: ByteSequence::from(input.payload),
            import_segments: Vec::new(),
            extrinsic: extrinsic_specs,
        }],
    };
    let canonical_work_package = work_package.encode();
    let package_hash = work_package.jam_hash().0;
    let report_input = WorkReportInput {
        core_index: stage0::CORE_INDEX,
        work_package: Arc::new(work_package.clone()),
        external_data: Arc::new(vec![external_data]),
        import_segments: Arc::new(vec![Vec::new()]),
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
                47, 98, 49, 26, 164, 41, 251, 58, 132, 97, 92, 153, 45, 86, 97, 46, 205, 111, 170,
                5, 41, 244, 133, 201, 146, 202, 66, 133, 104, 222, 222, 191,
            ]
        );
        assert_eq!(
            built.content_ref.content_hash,
            [
                93, 171, 219, 123, 150, 170, 129, 204, 82, 12, 80, 76, 10, 19, 3, 55, 149, 91, 139,
                119, 181, 241, 53, 138, 109, 169, 241, 130, 117, 136, 104, 96,
            ]
        );
        assert_eq!(
            built.content_ref.cid_v1.as_slice(),
            &[
                1, 85, 160, 228, 2, 32, 93, 171, 219, 123, 150, 170, 129, 204, 82, 12, 80, 76, 10,
                19, 3, 55, 149, 91, 139, 119, 181, 241, 53, 138, 109, 169, 241, 130, 117, 136, 104,
                96,
            ]
        );
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
