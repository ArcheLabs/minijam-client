use crate as pallet_minijam_bridge;
use frame_support::{
    assert_noop, assert_ok, derive_impl, parameter_types,
    traits::tokens::fungible::{Inspect, InspectHold},
};
use minijam_bridge_engine::admin_bridge_key;
use minijam_protocol::{AssetId, BridgeEffect};
use sp_runtime::traits::Convert;
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
    pub type Bridge = pallet_minijam_bridge::Pallet<Test>;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type AccountData = pallet_balances::AccountData<u128>;
}

parameter_types! {
    pub const ExistentialDeposit: u128 = 1;
    pub const EscrowAccount: u64 = 99;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
    type AccountStore = System;
    type Balance = u128;
    type ExistentialDeposit = ExistentialDeposit;
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
}

impl pallet_minijam_bridge::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type RuntimeHoldReason = RuntimeHoldReason;
    type AccountIdConverter = TestAccountIdConverter;
    type EscrowAccount = EscrowAccount;
}

pub struct TestAccountIdConverter;

impl Convert<u64, [u8; 32]> for TestAccountIdConverter {
    fn convert(account: u64) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[24..32].copy_from_slice(&account.to_be_bytes());
        bytes
    }
}

fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(1, 1_000), (2, 1), (99, 1)],
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .unwrap();
    storage.into()
}

#[test]
fn inbound_moves_funds_to_held_escrow() {
    new_test_ext().execute_with(|| {
        assert_ok!(Bridge::bridge_in(RuntimeOrigin::signed(1), 7, 100));
        let reason: RuntimeHoldReason = pallet_minijam_bridge::HoldReason::BridgeEscrow.into();

        assert_eq!(Balances::total_balance(&1), 900);
        assert_eq!(Balances::total_balance(&99), 101);
        assert_eq!(Balances::balance_on_hold(&reason, &99), 100);
        assert_eq!(pallet_minijam_bridge::NextInboundNonce::<Test>::get(), 1);
        assert!(pallet_minijam_bridge::InboundRecords::<Test>::get(0).is_some());
        let effect = BridgeEffect::Inbound {
            nonce: 0,
            target_service: 7,
            asset: AssetId::Native,
            amount: 100,
            account: TestAccountIdConverter::convert(1),
        };
        assert!(
            pallet_minijam_bridge::AdminBridgeRecords::<Test>::get(admin_bridge_key(&effect))
                .is_some()
        );
    });
}

#[test]
fn outbound_releases_once_from_escrow() {
    new_test_ext().execute_with(|| {
        assert_ok!(Bridge::bridge_in(RuntimeOrigin::signed(1), 7, 100));
        assert_ok!(Bridge::release_outbound(
            RuntimeOrigin::root(),
            42,
            2,
            7,
            40
        ));
        let reason: RuntimeHoldReason = pallet_minijam_bridge::HoldReason::BridgeEscrow.into();

        assert_eq!(Balances::total_balance(&2), 41);
        assert_eq!(Balances::balance_on_hold(&reason, &99), 60);
        assert!(pallet_minijam_bridge::ProcessedOutboundNonces::<Test>::contains_key(42));
        let effect = BridgeEffect::Outbound {
            nonce: 42,
            source_service: 7,
            asset: AssetId::Native,
            amount: 40,
            account: TestAccountIdConverter::convert(2),
        };
        assert!(
            pallet_minijam_bridge::AdminBridgeRecords::<Test>::get(admin_bridge_key(&effect))
                .is_some()
        );
        assert_noop!(
            Bridge::release_outbound(RuntimeOrigin::root(), 42, 2, 7, 1),
            pallet_minijam_bridge::Error::<Test>::OutboundAlreadyProcessed
        );
    });
}
