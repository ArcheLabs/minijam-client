use minijam_protocol::Hash;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalityObservation {
    pub finalized_block: Hash,
    pub finalized_number: u32,
}
