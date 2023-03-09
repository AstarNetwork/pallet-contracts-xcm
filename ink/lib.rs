#![cfg_attr(not(feature = "std"), no_std)]

use ink::env::{chain_extension::FromStatusCode, Environment};
use scale::{Decode, Encode};
pub use xcm::{VersionedMultiAsset, VersionedMultiLocation, VersionedResponse, VersionedXcm};

#[derive(Decode, Encode)]
pub enum Error {
    NoResponse = 1,
    ScaleError = 2,
}

impl FromStatusCode for Error {
    fn from_status_code(status_code: u32) -> Result<(), Self> {
        match status_code {
            0 => Ok(()),
            1 => Err(Self::NoResponse),
            _ => panic!("Unknown status code"),
        }
    }
}

impl From<scale::Error> for Error {
    fn from(_value: scale::Error) -> Self {
        Self::ScaleError
    }
}

#[ink::chain_extension]
pub trait XCMExtension {
    type ErrorCode = Error;

    #[ink(extension = 0x00A0000, handle_status = false)]
    fn prepare_execute(xcm: VersionedXcm<()>) -> u64;

    #[ink(extension = 0x00A0001, handle_status = false)]
    fn execute();

    #[ink(extension = 0x00A0002, handle_status = false)]
    fn prepare_send(dest: VersionedMultiLocation, xcm: VersionedXcm<()>) -> VersionedMultiAsset;

    #[ink(extension = 0x00A0003, handle_status = false)]
    fn send();

    #[ink(extension = 0x00A0004, handle_status = false)]
    fn new_query() -> u64;

    #[ink(extension = 0x00A0005, handle_status = true)]
    fn take_response(query_id: u64) -> Result<VersionedResponse, Error>;
}

pub enum CustomEnvironment {}

impl Environment for CustomEnvironment {
    const MAX_EVENT_TOPICS: usize = <ink::env::DefaultEnvironment as Environment>::MAX_EVENT_TOPICS;

    type AccountId = <ink::env::DefaultEnvironment as Environment>::AccountId;
    type Balance = <ink::env::DefaultEnvironment as Environment>::Balance;
    type Hash = <ink::env::DefaultEnvironment as Environment>::Hash;
    type BlockNumber = <ink::env::DefaultEnvironment as Environment>::BlockNumber;
    type Timestamp = <ink::env::DefaultEnvironment as Environment>::Timestamp;

    type ChainExtension = XCMExtension;
}

#[ink::contract(env = crate::CustomEnvironment)]
mod xcm_contract_poc {
    use ink::prelude::vec::Vec;
    // use scale::Encode;
    // use sp_runtime::traits::Get;
    pub use xcm::opaque::latest::prelude::{
        Junction, Junctions::X1, MultiLocation, NetworkId::Any, OriginKind, Transact, Xcm, *,
    };
    pub use xcm::{VersionedMultiAsset, VersionedMultiLocation, VersionedResponse, VersionedXcm};
    // use xcm_builder::Account32Hash;
    // use xcm_executor::traits::Convert;

    // pub type NativeAccountId = <<sp_runtime::MultiSignature as sp_runtime::traits::Verify>::Signer as sp_runtime::traits::IdentifyAccount>::AccountId;

    #[ink(storage)]
    #[derive(Default)]
    pub struct XcmContractPoC;

    // struct AnyNetwork;
    // impl Get<NetworkId> for AnyNetwork {
    //     fn get() -> NetworkId {
    //         NetworkId::Any
    //     }
    // }

    impl XcmContractPoC {
        #[ink(constructor)]
        pub fn default() -> Self {
            Default::default()
        }

        // #[ink(message)]
        // pub fn derived_address(
        //     &self,
        //     parachain_id: Option<u32>,
        //     network: NetworkId,
        // ) -> NativeAccountId {
        //     let mut loc = MultiLocation::parent();

        //     if let Some(parachain_id) = parachain_id {
        //         loc.append_with(X1(Parachain(parachain_id))).unwrap();
        //     }

        //     loc.append_with(X1(AccountId32 {
        //         network,
        //         id: self.env().account_id().encode().try_into().unwrap(),
        //     }))
        //     .unwrap();

        //     Account32Hash::<AnyNetwork, NativeAccountId>::convert_ref(&loc).unwrap()
        // }

        #[ink(message)]
        pub fn send_message(
            &mut self,
            para_id: u32,
            call: Vec<u8>,
            max_fees: Option<u128>,
            max_weight: Option<u64>,
        ) {
            // let caller: [u8; 32] = self.env().caller().encode().try_into().unwrap();
            let max_weight = max_weight.unwrap_or(1_000_000_000);
            let max_fees = max_fees.unwrap_or(100_000_000_000_000_000);

            let multi_location = VersionedMultiLocation::V1(MultiLocation {
                parents: 1,
                interior: X1(Junction::Parachain(para_id)),
            });
            let versioned_xcm = VersionedXcm::from(Xcm([
                // DescendOrigin(Junctions::X1(Junction::AccountId32 {
                //     network: Kusama,
                //     id: caller,
                // })),
                WithdrawAsset((Junctions::Here, max_fees).into()),
                BuyExecution {
                    fees: (Here, max_fees).into(),
                    weight_limit: Unlimited,
                },
                Transact {
                    origin_type: OriginKind::SovereignAccount,
                    require_weight_at_most: max_weight,
                    call: call.into(),
                },
                RefundSurplus,
            ]
            .to_vec()));

            self.env()
                .extension()
                .prepare_send(multi_location, versioned_xcm);
            self.env().extension().send();
        }
    }
}
