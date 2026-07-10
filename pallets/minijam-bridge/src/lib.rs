// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{
        pallet_prelude::*,
        traits::tokens::{
            fungible::{Inspect, Mutate, MutateHold},
            Precision, Preservation,
        },
        transactional,
    };
    use frame_system::pallet_prelude::*;
    use minijam_bridge_engine::{admin_bridge_key, admin_bridge_value};
    use minijam_protocol::{AssetId, BridgeEffect, StateValue};
    use sp_runtime::traits::{Convert, SaturatedConversion, Zero};

    pub type BalanceOf<T> =
        <<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

    #[pallet::composite_enum]
    pub enum HoldReason {
        BridgeEscrow,
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    #[scale_info(skip_type_params(T))]
    pub struct InboundRecord<T: Config> {
        pub nonce: u64,
        pub account: T::AccountId,
        pub target_service: u32,
        pub asset: AssetId,
        pub amount: BalanceOf<T>,
    }

    #[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
    #[scale_info(skip_type_params(T))]
    pub struct OutboundRecord<T: Config> {
        pub nonce: u64,
        pub account: T::AccountId,
        pub source_service: u32,
        pub asset: AssetId,
        pub amount: BalanceOf<T>,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Currency: Inspect<Self::AccountId>
            + Mutate<Self::AccountId>
            + MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>;

        type RuntimeHoldReason: From<HoldReason>;

        type AccountIdConverter: Convert<Self::AccountId, [u8; 32]>;

        #[pallet::constant]
        type EscrowAccount: Get<Self::AccountId>;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type NextInboundNonce<T> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    pub type InboundRecords<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, InboundRecord<T>, OptionQuery>;

    #[pallet::storage]
    pub type ProcessedOutboundNonces<T> = StorageMap<_, Blake2_128Concat, u64, (), OptionQuery>;

    #[pallet::storage]
    pub type OutboundRecords<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, OutboundRecord<T>, OptionQuery>;

    #[pallet::storage]
    pub type AdminBridgeRecords<T> =
        StorageMap<_, Blake2_128Concat, [u8; 31], StateValue, OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        InboundEscrowed {
            nonce: u64,
            account: T::AccountId,
            target_service: u32,
            amount: BalanceOf<T>,
        },
        OutboundReleased {
            nonce: u64,
            account: T::AccountId,
            source_service: u32,
            amount: BalanceOf<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        ZeroAmount,
        NonceOverflow,
        OutboundAlreadyProcessed,
        BridgeRecordTooLarge,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::DbWeight::get().reads_writes(2, 4))]
        #[transactional]
        pub fn bridge_in(
            origin: OriginFor<T>,
            target_service: u32,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            let account = ensure_signed(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);

            let nonce = NextInboundNonce::<T>::get();
            let next = nonce.checked_add(1).ok_or(Error::<T>::NonceOverflow)?;
            let escrow = T::EscrowAccount::get();
            T::Currency::transfer(&account, &escrow, amount, Preservation::Preserve)?;
            T::Currency::hold(
                &T::RuntimeHoldReason::from(HoldReason::BridgeEscrow),
                &escrow,
                amount,
            )?;

            InboundRecords::<T>::insert(
                nonce,
                InboundRecord::<T> {
                    nonce,
                    account: account.clone(),
                    target_service,
                    asset: AssetId::Native,
                    amount,
                },
            );
            let effect = BridgeEffect::Inbound {
                nonce,
                target_service,
                asset: AssetId::Native,
                amount: amount.saturated_into(),
                account: T::AccountIdConverter::convert(account.clone()),
            };
            let value =
                admin_bridge_value(&effect).map_err(|_| Error::<T>::BridgeRecordTooLarge)?;
            AdminBridgeRecords::<T>::insert(admin_bridge_key(&effect), value);
            NextInboundNonce::<T>::put(next);
            Self::deposit_event(Event::InboundEscrowed {
                nonce,
                account,
                target_service,
                amount,
            });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::DbWeight::get().reads_writes(2, 4))]
        #[transactional]
        pub fn release_outbound(
            origin: OriginFor<T>,
            nonce: u64,
            account: T::AccountId,
            source_service: u32,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
            ensure!(
                !ProcessedOutboundNonces::<T>::contains_key(nonce),
                Error::<T>::OutboundAlreadyProcessed
            );

            let escrow = T::EscrowAccount::get();
            T::Currency::release(
                &T::RuntimeHoldReason::from(HoldReason::BridgeEscrow),
                &escrow,
                amount,
                Precision::Exact,
            )?;
            T::Currency::transfer(&escrow, &account, amount, Preservation::Preserve)?;

            ProcessedOutboundNonces::<T>::insert(nonce, ());
            OutboundRecords::<T>::insert(
                nonce,
                OutboundRecord::<T> {
                    nonce,
                    account: account.clone(),
                    source_service,
                    asset: AssetId::Native,
                    amount,
                },
            );
            let effect = BridgeEffect::Outbound {
                nonce,
                source_service,
                asset: AssetId::Native,
                amount: amount.saturated_into(),
                account: T::AccountIdConverter::convert(account.clone()),
            };
            let value =
                admin_bridge_value(&effect).map_err(|_| Error::<T>::BridgeRecordTooLarge)?;
            AdminBridgeRecords::<T>::insert(admin_bridge_key(&effect), value);
            Self::deposit_event(Event::OutboundReleased {
                nonce,
                account,
                source_service,
                amount,
            });
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
