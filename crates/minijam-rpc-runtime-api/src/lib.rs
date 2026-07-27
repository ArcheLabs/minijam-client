// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use minijam_protocol::{Hash, WorkId, WorkerTaskV1, WorkerVoteTaskV1};
use sp_api::decl_runtime_apis;

decl_runtime_apis! {
    pub trait MiniJamRuntimeApi {
        fn get_work(work_id: WorkId) -> Option<Vec<u8>>;
        fn get_pending_work_tasks() -> Vec<WorkerTaskV1>;
        fn get_open_vote_tasks() -> Vec<WorkerVoteTaskV1>;
        fn get_work_by_package_hash(package_hash: Hash) -> Option<Vec<u8>>;
        fn get_work_bundle_ref(work_id: WorkId) -> Option<Vec<u8>>;
        fn get_candidate(work_id: WorkId, round: u8) -> Option<Vec<u8>>;
        fn get_execution_receipt(work_id: WorkId) -> Option<Hash>;
        fn get_last_execution_receipt() -> Option<Hash>;
        fn get_service_fuel(service_id: u32) -> Vec<u8>;
        fn get_work_fuel_reservation(work_id: WorkId) -> Option<Vec<u8>>;
        fn get_work_fuel_settlement(work_id: WorkId) -> Option<Vec<u8>>;
        fn get_pending_preimages() -> Vec<u8>;
        fn get_quarantined_preimages() -> Vec<u8>;
        fn has_pending_preimage(requester: u32, blob_hash: Hash, blob_len: u32) -> bool;
        fn get_pending_system_ops() -> Vec<u8>;
        fn get_quarantined_system_ops() -> Vec<u8>;
        fn get_system_op(request_id: Hash) -> Option<Vec<u8>>;
        fn get_system_receipt(request_id: Hash) -> Option<Vec<u8>>;
        fn get_system_op_nonce(sender: [u8; 32]) -> u64;
        fn get_system_service_info() -> Option<Vec<u8>>;
        fn get_service_info(service_id: u32) -> Option<Vec<u8>>;
        fn get_service_storage(service_id: u32, key: Vec<u8>) -> Option<Vec<u8>>;
        fn get_service_preimage(service_id: u32, code_hash: Hash) -> Option<Vec<u8>>;
        fn get_service_controller(service_id: u32) -> Option<Vec<u8>>;
        fn get_protocol_state(key: [u8; 31]) -> Option<Vec<u8>>;
    }
}
