use frame_system;
use minijam_protocol::Hash;
use minijam_runtime::RuntimeCall;
use parity_scale_codec::Encode;
use sp_core::{sr25519, Pair, H256};
use sp_runtime::{generic::Era, traits::IdentifyAccount, MultiSigner};

pub fn sign_call(
    pair: &sr25519::Pair,
    nonce: u32,
    genesis_hash: Hash,
    call: RuntimeCall,
) -> Vec<u8> {
    let tx_ext = (
        frame_system::AuthorizeCall::<minijam_runtime::Runtime>::new(),
        frame_system::CheckNonZeroSender::<minijam_runtime::Runtime>::new(),
        frame_system::CheckSpecVersion::<minijam_runtime::Runtime>::new(),
        frame_system::CheckTxVersion::<minijam_runtime::Runtime>::new(),
        frame_system::CheckGenesis::<minijam_runtime::Runtime>::new(),
        frame_system::CheckEra::<minijam_runtime::Runtime>::from(Era::Immortal),
        frame_system::CheckNonce::<minijam_runtime::Runtime>::from(nonce),
        frame_system::CheckWeight::<minijam_runtime::Runtime>::new(),
        pallet_transaction_payment::ChargeTransactionPayment::<minijam_runtime::Runtime>::from(0),
        frame_system::WeightReclaim::<minijam_runtime::Runtime>::new(),
    );
    let genesis_hash = H256::from(genesis_hash);
    let payload = minijam_runtime::SignedPayload::from_raw(
        call.clone(),
        tx_ext.clone(),
        (
            (),
            (),
            minijam_runtime::VERSION.spec_version,
            minijam_runtime::VERSION.transaction_version,
            genesis_hash,
            genesis_hash,
            (),
            (),
            (),
            (),
        ),
    );
    let signature = payload.using_encoded(|bytes| pair.sign(bytes));
    let signer = MultiSigner::Sr25519(pair.public()).into_account();
    minijam_runtime::UncheckedExtrinsic::new_signed(
        call,
        minijam_runtime::Address::Id(signer),
        minijam_runtime::Signature::Sr25519(signature),
        tx_ext,
    )
    .encode()
}

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::Decode;

    #[test]
    fn signed_runtime_calls_commit_the_allocated_nonce() {
        let pair = sr25519::Pair::from_seed(&[7; 32]);
        let call = RuntimeCall::MiniJam(pallet_minijam::Call::submit_preimage {
            canonical_preimage: vec![1, 2, 3].try_into().unwrap(),
        });

        let first = sign_call(&pair, 4, [9; 32], call.clone());
        let second = sign_call(&pair, 5, [9; 32], call);

        assert_ne!(first, second);
        assert!(minijam_runtime::UncheckedExtrinsic::decode(&mut first.as_slice()).is_ok());
        assert!(minijam_runtime::UncheckedExtrinsic::decode(&mut second.as_slice()).is_ok());
    }
}
