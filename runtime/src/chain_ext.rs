use core::fmt::Debug;

use crate::{Config, Error as PalletError};
use codec::{Decode, Encode, HasCompact};
use frame_support::{traits::Currency, weights::Weight, DefaultNoBound};
use log;
use pallet_contracts::{
	chain_extension::{
		ChainExtension, Environment, Ext, InitState, RegisteredChainExtension,
		Result as DispatchResult, RetVal, SysConfig,
	},
	Call as PalletContractCall,
};
use scale_info::TypeInfo;
use sp_runtime::traits::{Bounded, StaticLookup};
use sp_std::prelude::*;
use xcm::prelude::*;
use xcm_executor::traits::{InvertLocation, WeightBounds};

type RuntimeCallOf<T> = <T as SysConfig>::RuntimeCall;

#[repr(u16)]
#[derive(num_enum::TryFromPrimitive)]
enum Command {
	PrepareExecute = 0,
	Execute = 1,
	ValidateSend = 2,
	Send = 3,
	NewQuery = 4,
	TakeResponse = 5,
	ContractCall = 6,
}

#[repr(u32)]
#[derive(num_enum::IntoPrimitive)]
enum Error {
	Success = 0,
	NoResponse = 1,
}

#[derive(Decode)]
struct ValidateSendInput {
	dest: VersionedMultiLocation,
	xcm: VersionedXcm<()>,
}

#[derive(Decode)]
struct ContractCallInput {
	// contract multilocation in context of caller
	dest: VersionedMultiLocation,
	data: Vec<u8>,
	// contract call gas limit
	gas_limit: Weight,
	// fee multiasset in context of caller
	max_fees: VersionedMultiAsset,
	// max weight for XCM transact
	max_weight: u64,
}

pub struct PreparedExecution<Call> {
	xcm: Xcm<Call>,
	weight: Weight,
}

pub struct ValidatedSend {
	dest: MultiLocation,
	xcm: Xcm<()>,
}

#[derive(DefaultNoBound)]
pub struct Extension<T: Config> {
	prepared_execute: Option<PreparedExecution<RuntimeCallOf<T>>>,
	validated_send: Option<ValidatedSend>,
}

macro_rules! unwrap {
	($val:expr, $err:expr) => {
		match $val {
			Ok(inner) => inner,
			Err(_) => return Ok(RetVal::Converging($err.into())),
		}
	};
}

impl<T: Config> ChainExtension<T> for Extension<T>
where
	<T as SysConfig>::AccountId: AsRef<[u8; 32]>,
	<<<T as pallet_contracts::Config>::Currency as Currency<<T as SysConfig>::AccountId>>::Balance as HasCompact>::Type: Clone + Encode + TypeInfo + Debug + Eq,
	<T as pallet_contracts::Config>::RuntimeCall: From<pallet_contracts::Call<T>> + Encode,
{
	fn call<E>(&mut self, mut env: Environment<E, InitState>) -> DispatchResult<RetVal>
	where
		E: Ext<T = T>,
	{
		match Command::try_from(env.func_id()).map_err(|_| PalletError::<T>::InvalidCommand)? {
			Command::PrepareExecute => {
				log::trace!(target: "xcm::send_xcm", "76");
				let mut env = env.buf_in_buf_out();
				log::trace!(target: "xcm::send_xcm", "78");
				let len = env.in_len();
				log::trace!(target: "xcm::send_xcm", "80");
				let input: VersionedXcm<RuntimeCallOf<T>> = env.read_as_unbounded(len)?;
				log::trace!(target: "xcm::send_xcm", "input: {:?}", &input);
				let mut xcm =
					input.try_into().map_err(|_| PalletError::<T>::XcmVersionNotSupported)?;
				log::trace!(target: "xcm::send_xcm", "XCM: {:?}", &xcm);
				let weight = Weight::from_ref_time(
					T::Weigher::weight(&mut xcm).map_err(|_| PalletError::<T>::CannotWeigh)?,
				);
				log::trace!(target: "xcm::send_xcm", "89");
				self.prepared_execute = Some(PreparedExecution { xcm, weight });
				log::trace!(target: "xcm::send_xcm", "91");
				weight.using_encoded(|w| env.write(w, true, None))?;
				log::trace!(target: "xcm::send_xcm", "93");
			},
			Command::Execute => {
				let input = self
					.prepared_execute
					.as_ref()
					.take()
					.ok_or(PalletError::<T>::PreparationMissing)?;
				env.charge_weight(input.weight)?;
				let origin = MultiLocation {
					parents: 0,
					interior: Junctions::X1(Junction::AccountId32 {
						network: NetworkId::Any,
						id: *env.ext().address().as_ref(),
					}),
				};
				let outcome = T::XcmExecutor::execute_xcm_in_credit(
					origin,
					input.xcm.clone(),
					input.weight.ref_time(),
					input.weight.ref_time(),
				);
				// revert for anything but a complete excution
				match outcome {
					Outcome::Complete(_) => (),
					_ => Err(PalletError::<T>::ExecutionFailed)?,
				}
			},
			Command::ValidateSend => {
				let mut env = env.buf_in_buf_out();
				let len = env.in_len();
				let input: ValidateSendInput = env.read_as_unbounded(len)?;
				// just a dummy asset until XCMv3 rolls around with its validate function
				let asset = self.validate_send(input)?;
				VersionedMultiAsset::from(asset).using_encoded(|a| env.write(a, true, None))?;
			},
			Command::Send => {
				let caller = *env.ext().caller().as_ref();
				self.send(caller)?;
			},
			Command::NewQuery => {
				let mut env = env.buf_in_buf_out();
				let location = MultiLocation {
					parents: 0,
					interior: Junctions::X1(Junction::AccountId32 {
						network: NetworkId::Any,
						id: *env.ext().address().as_ref(),
					}),
				};
				let query_id: u64 =
					pallet_xcm::Pallet::<T>::new_query(location, Bounded::max_value()).into();
				query_id.using_encoded(|q| env.write(q, true, None))?;
			},
			Command::TakeResponse => {
				let mut env = env.buf_in_buf_out();
				let query_id: u64 = env.read_as()?;
				let response = unwrap!(
					pallet_xcm::Pallet::<T>::take_response(query_id).map(|ret| ret.0).ok_or(()),
					Error::NoResponse
				);
				VersionedResponse::from(response).using_encoded(|r| env.write(r, true, None))?;
			},
			Command::ContractCall => {
				log::trace!(target: "xcm::contract_call", "CALLED");

				let mut env = env.buf_in_buf_out();
				let len = env.in_len();
				let ContractCallInput { data, dest, gas_limit, max_fees,max_weight } =
					env.read_as_unbounded(len)?;

				let dest: MultiLocation =
					dest.try_into().map_err(|_| PalletError::<T>::XcmVersionNotSupported)?;
				let max_fees: MultiAsset = max_fees.try_into().map_err(|_| PalletError::<T>::XcmVersionNotSupported)?;


				// split destination chain and contract location
				let (dest, contract_dest) = dest.split_last_interior();
				// extract the contract address
				let contract_account = match contract_dest {
					Some(AccountId32 { id, .. }) => id,
					_ => {
						return Err(PalletError::<T>::InvalidCommand.into());
					},
				};

				// convert the fee asset for dest context
				let ancestry = T::LocationInverter::ancestry();
				let max_fees = max_fees
					.reanchored(&dest, &ancestry)
					.map_err(|_| PalletError::<T>::XcmVersionNotSupported)?;

				log::trace!(target: "xcm::contract_call", "max_fees={max_fees:?}, dest={dest:?}");

				let call: <T as pallet_contracts::Config>::RuntimeCall = PalletContractCall::<T>::call {
					// Convert the AccountId32 into Lookup
					// it is ugly, no doubt
					dest: <<T as SysConfig>::Lookup as StaticLookup>::unlookup(Decode::decode(&mut contract_account.as_slice()).unwrap()),
					value: env.ext().value_transferred(),
					gas_limit,
					storage_deposit_limit: None,
					data,
				}.into();

				log::trace!(target: "xcm::contract_call", "enoded_call={}", hex::encode(call.encode()));

				// build the XCM message
				let xcm = VersionedXcm::from(Xcm([
					// withdraw the fee asset
					WithdrawAsset(max_fees.clone().into()),
					// buy the max weight we can
					BuyExecution {
						fees: max_fees.into(),
						weight_limit: Unlimited,
					},
					// run the extrinsic (in this case pallet contracts call)
					Transact {
						origin_type: OriginKind::SovereignAccount,
						require_weight_at_most: max_weight,
						call: call.encode().into(),
					},
					// refund the remainind fees
					RefundSurplus,
				]
				.to_vec()));

				self.validate_send(ValidateSendInput { dest: xcm::VersionedMultiLocation::V1(dest), xcm })?;
				let caller = *env.ext().caller().as_ref();
				self.send(caller)?;
			},
		}

		Ok(RetVal::Converging(Error::Success.into()))
	}
}

impl<T: Config> RegisteredChainExtension<T> for Extension<T>
where
	<T as SysConfig>::AccountId: AsRef<[u8; 32]> ,
	<<<T as pallet_contracts::Config>::Currency as Currency<<T as SysConfig>::AccountId>>::Balance as HasCompact>::Type: Clone + Encode + TypeInfo + Debug + Eq,
	<T as pallet_contracts::Config>::RuntimeCall: From<pallet_contracts::Call<T>> + Encode,
{
	const ID: u16 = 10;
}

impl<T: Config> Extension<T> {
	fn validate_send(&mut self, input: ValidateSendInput) -> Result<MultiAsset, PalletError<T>> {
		self.validated_send = Some(ValidatedSend {
			dest: input.dest.try_into().map_err(|_| PalletError::<T>::XcmVersionNotSupported)?,
			xcm: input.xcm.try_into().map_err(|_| PalletError::<T>::XcmVersionNotSupported)?,
		});
		// just a dummy asset until XCMv3 rolls around with its validate function
		let asset = MultiAsset {
			id: AssetId::Concrete(MultiLocation { parents: 0, interior: Junctions::Here }),
			fun: Fungibility::Fungible(0),
		};
		Ok(asset)
	}

	fn send(&mut self, caller: [u8; 32]) -> Result<(), PalletError<T>> {
		let input = self
			.validated_send
			.as_ref()
			.take()
			.ok_or(PalletError::<T>::PreparationMissing)?;
		log::trace!(target: "xcm::send_xcm", "Input validated_send");
		pallet_xcm::Pallet::<T>::send_xcm(
			Junctions::X1(Junction::AccountId32 { network: NetworkId::Any, id: caller }),
			input.dest.clone(),
			input.xcm.clone(),
		)
		.map_err(|e| {
			log::debug!(
				target: "Contracts",
				"Send Failed: {:?}",
				e
			);
			PalletError::<T>::SendFailed
		})?;
		log::trace!(target: "xcm::send_xcm", "CALLED");

		Ok(())
	}
}
