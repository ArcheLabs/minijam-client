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
    use minijam_protocol::SystemCommandV2;
    use parity_scale_codec::{Decode, Encode};
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct CreateServiceVector {
        encoded_extrinsic: String,
    }

    fn decode_hex(value: &str) -> Vec<u8> {
        value
            .trim_start_matches("0x")
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let text = std::str::from_utf8(pair).expect("hex");
                u8::from_str_radix(text, 16).expect("hex byte")
            })
            .collect()
    }

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

    #[test]
    fn create_service_extrinsic_decodes_without_trailing_bytes() {
        let pair = sr25519::Pair::from_seed(&[7; 32]);
        let code_hash = [0x11; 32];
        let call = RuntimeCall::MiniJam(pallet_minijam::Call::submit_system_op {
            command: Box::new(SystemCommandV2::CreateService {
                code_hash,
                code_len: 69_414,
                min_item_gas: 1,
                min_memo_gas: 1,
            }),
        });
        let bytes = sign_call(&pair, 0, [0x22; 32], call);
        let mut input = bytes.as_slice();
        let decoded = minijam_runtime::UncheckedExtrinsic::decode(&mut input)
            .expect("decode exact CreateService extrinsic");
        assert!(input.is_empty(), "decoder left trailing bytes");
        assert_eq!(decoded.encode(), bytes);
        let vector: CreateServiceVector = serde_json::from_str(include_str!(
            "../../../test-vectors/create-service-extrinsic-v1.json"
        ))
        .expect("golden CreateService vector");
        let golden_bytes = decode_hex(&vector.encoded_extrinsic);
        let mut golden_input = golden_bytes.as_slice();
        let golden = minijam_runtime::UncheckedExtrinsic::decode(&mut golden_input)
            .expect("decode golden CreateService extrinsic");
        assert!(golden_input.is_empty());

        match decoded.function {
            RuntimeCall::MiniJam(pallet_minijam::Call::submit_system_op { command }) => {
                assert_eq!(
                    *command,
                    SystemCommandV2::CreateService {
                        code_hash,
                        code_len: 69_414,
                        min_item_gas: 1,
                        min_memo_gas: 1,
                    }
                );
            }
            other => panic!("unexpected decoded call: {other:?}"),
        }
        match golden.function {
            RuntimeCall::MiniJam(pallet_minijam::Call::submit_system_op { command }) => {
                assert_eq!(
                    *command,
                    SystemCommandV2::CreateService {
                        code_hash,
                        code_len: 69_414,
                        min_item_gas: 1,
                        min_memo_gas: 1,
                    }
                );
            }
            other => panic!("unexpected golden decoded call: {other:?}"),
        }
    }
}
