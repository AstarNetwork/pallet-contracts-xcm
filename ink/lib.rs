#![cfg_attr(not(feature = "std"), no_std)]

use ink_env::{chain_extension::FromStatusCode, Environment};
use ink_lang as ink;
use scale::Decode;
pub use xcm::{VersionedMultiAsset, VersionedMultiLocation, VersionedResponse, VersionedXcm};

#[derive(Decode)]
pub enum Error {
    NoResponse = 1,
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

#[ink::chain_extension]
pub trait XCMExtension {
    type ErrorCode = Error;

    #[ink(extension = 0x00010000, handle_status = false, returns_result = false)]
    fn prepare_execute(xcm: VersionedXcm<()>) -> u64;

    #[ink(extension = 0x00010001, handle_status = false, returns_result = false)]
    fn execute();

    #[ink(extension = 0x00010002, handle_status = false, returns_result = false)]
    fn prepare_send(dest: VersionedMultiLocation, xcm: VersionedXcm<()>) -> VersionedMultiAsset;

    #[ink(extension = 0x00010003, handle_status = false, returns_result = false)]
    fn send();

    #[ink(extension = 0x00010004, handle_status = false, returns_result = false)]
    fn new_query() -> u64;

    #[ink(extension = 0x00010005, handle_status = true, returns_result = false)]
    fn take_response(query_id: u64) -> Result<VersionedResponse, Error>;
}

pub enum CustomEnvironment {}

impl Environment for CustomEnvironment {
    const MAX_EVENT_TOPICS: usize = <ink_env::DefaultEnvironment as Environment>::MAX_EVENT_TOPICS;

    type AccountId = <ink_env::DefaultEnvironment as Environment>::AccountId;
    type Balance = <ink_env::DefaultEnvironment as Environment>::Balance;
    type Hash = <ink_env::DefaultEnvironment as Environment>::Hash;
    type BlockNumber = <ink_env::DefaultEnvironment as Environment>::BlockNumber;
    type Timestamp = <ink_env::DefaultEnvironment as Environment>::Timestamp;

    type ChainExtension = XCMExtension;
}

#[ink::contract(env = crate::CustomEnvironment)]
mod xcm_contract_poc {
    use ink_env::call::Call;
    pub use xcm::opaque::latest::prelude::{
        Junction, Junctions::X1, MultiLocation, NetworkId::Any, OriginKind, Transact, Xcm, *,
    };
    //pub use xcm::opaque::latest::prelude::*;
    use ink_prelude::vec::Vec;
    pub use xcm::{VersionedMultiAsset, VersionedMultiLocation, VersionedResponse, VersionedXcm};
    /// Defines the storage of your contract.
    /// Add new fields to the below struct in order
    /// to add new static storage fields to your contract.
    #[ink(storage)]
    pub struct XcmContractPoC {
        value: bool,
    }

    impl XcmContractPoC {
        /// Constructor that initializes the `bool` value to the given `init_value`.
        #[ink(constructor)]
        pub fn new(value: bool) -> Self {
            Self { value }
        }

        /// Constructor that initializes the `bool` value to `false`.
        ///
        /// Constructors can delegate to other constructors.
        #[ink(constructor)]
        pub fn default() -> Self {
            Self::new(Default::default())
        }

        #[ink(message)]
        pub fn prepare_message(&mut self, para_id: u32, call: Vec<u8>) {
            let multi_location = VersionedMultiLocation::V1(MultiLocation {
                parents: 1,
                interior: X1(Junction::Parachain(para_id)),
            });
            let versioned_xcm: xcm::VersionedXcm<Call> = VersionedXcm::from(Xcm([
                WithdrawAsset((Junctions::Here, 100_000_000_000).into()),
                BuyExecution {
                    fees: (Here, 100_000_000_000).into(),
                    weight_limit: Unlimited
                },
                Transact {
                    origin_type: OriginKind::SovereignAccount,
                    require_weight_at_most: 1_000_000_000 as u64,
                    call: call.into(),
                },
            ]
                .to_vec()));

            // self.env().extension().prepare_send(multi_location, versioned_xcm);
        }

        #[ink(message)]
        pub fn send_message(&mut self, para_id: u32, call: Vec<u8>) {
            let multi_location = VersionedMultiLocation::V1(MultiLocation {
                parents: 1,
                interior: X1(Junction::Parachain(para_id)),
            });
            let versioned_xcm = VersionedXcm::from(Xcm([
                WithdrawAsset((Junctions::Here, 100_000_000_000).into()),
                BuyExecution {
                    fees: (Here, 100_000_000_000).into(),
                    weight_limit: Unlimited
                },
                Transact {
                    origin_type: OriginKind::SovereignAccount,
                    require_weight_at_most: 1_000_000_000 as u64,
                    call: call.into(),
                },
            ]
                .to_vec()));

            self.env().extension().prepare_send(multi_location, versioned_xcm);
            self.env().extension().send();
        }
    }
}