#![cfg_attr(not(feature = "std"), no_std)]

use ink::env::{chain_extension::FromStatusCode, Environment};
use ink::prelude::vec::Vec;

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

    #[ink(extension = 0x00A0006, handle_status = true)]
    fn contract_call(
        dest: VersionedMultiLocation,
        data: Vec<u8>,
        gas_limit: sp_weights::Weight,
        max_fees: VersionedMultiAsset,
        max_weight: u64,
    ) -> u64;
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
    use scale::Encode;
    use sp_weights::Weight;

    pub use xcm::opaque::latest::prelude::{
        Junction, Junctions::X1, MultiLocation, NetworkId::Any, OriginKind, Transact, Xcm, *,
    };
    pub use xcm::{VersionedMultiAsset, VersionedMultiLocation, VersionedResponse, VersionedXcm};

    #[ink(storage)]
    #[derive(Default)]
    pub struct XcmContractPoC;

    impl XcmContractPoC {
        #[ink(constructor)]
        pub fn new() -> Self {
            Self {}
        }

        #[ink(message)]
        pub fn flip_sby(
            &mut self,
            // Parachain Id
            para_id: u32,
            // Contract Account
            account: AccountId,
            // Gas limit for contract call
            gas_limit: Weight,
            // Max fees for xcm call
            max_fees_amount: u128,
            // Max weight for xcm call
            max_weight: u64,
            // Optional selector for flip() function
            selector: Option<[u8; 4]>,
        ) {
            let mut data: Vec<u8> = Vec::new();
            // selctor for `flip()` method
            let mut selector: Vec<u8> = selector.unwrap_or([0x63, 0x3a, 0xa5, 0x51]).into();
            let account = account.encode().try_into().unwrap();
            data.append(&mut selector);

            let fee_asset = (Parent, Parachain(para_id));
            let contract_dest = (
                Parent,
                Parachain(para_id),
                AccountId32 {
                    network: Any,
                    id: account,
                },
            );

            let dest = contract_dest.into();
            let max_fees = (fee_asset, max_fees_amount).into();

            let _ = self
                .env()
                .extension()
                .contract_call(dest, data, gas_limit, max_fees, max_weight);
        }

        #[ink(message)]
        pub fn send_message(
            &mut self,
            para_id: u32,
            call: Vec<u8>,
            max_fees: Option<u128>,
            max_weight: Option<u64>,
        ) {
            let max_weight = max_weight.unwrap_or(1_000_000_000);
            let max_fees = max_fees.unwrap_or(100_000_000_000_000_000);

            let multi_location = VersionedMultiLocation::V1(MultiLocation {
                parents: 1,
                interior: X1(Junction::Parachain(para_id)),
            });
            let versioned_xcm = VersionedXcm::from(Xcm([
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
