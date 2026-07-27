use minijam_protocol::Hash;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalityObservation {
    pub finalized_block: Hash,
    pub finalized_number: u32,
}

#[derive(Clone, Debug)]
pub struct FinalizedEvent {
    pub block_hash: Hash,
    pub block_number: u32,
    pub event: minijam_runtime::RuntimeEvent,
}
