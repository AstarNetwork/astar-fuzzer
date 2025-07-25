use crate::invariants_check;
use astar_primitives::evm::H256;
use astar_primitives::genesis::GenesisAccount;
use astar_primitives::AccountId;
use astar_runtime::{
    Executive, ParachainInfo, Runtime, RuntimeBlockWeights, RuntimeCall, RuntimeOrigin,
    UncheckedExtrinsic, SLOT_DURATION,
};
use codec::{DecodeLimit, Encode};
use cumulus_primitives_core::relay_chain::Header as RelayHeader;
use cumulus_primitives_core::relay_chain::{HeadData, Slot};
use cumulus_primitives_core::Weight;
use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
use frame_support::dispatch::{DispatchClass, GetDispatchInfo};
use frame_support::traits::Get;
use frame_system::Account;
use pallet_balances::TotalIssuance;
use sp_consensus_aura::AURA_ENGINE_ID;
use sp_core::sr25519::Public;
use sp_core::storage::Storage;
use sp_runtime::traits::{Dispatchable, Header};
use sp_runtime::{Digest, DigestItem};
use sp_state_machine::BasicExternalities;
use std::iter;
use std::time::{Duration, Instant};

/// Processing fuzzer: executing extrinsics in blocks.
pub fn process_input(accounts: &[GenesisAccount<Public>], genesis: &Storage, data: &[u8]) {
    let mut extrinsic_data = data;

    let extrinsics: Vec<(u8, u8, RuntimeCall)> =
        iter::from_fn(|| DecodeLimit::decode_with_depth_limit(64, &mut extrinsic_data).ok())
            .filter(|(_, _, x): &(_, _, RuntimeCall)| {
                !recursively_find_call(x.clone(), call_filter)
            })
            .collect();

    let mut block: u32 = 1;
    let mut weight: Weight = Weight::zero();
    let mut elapsed: Duration = Duration::ZERO;

    BasicExternalities::execute_with_storage(&mut genesis.clone(), || {
        let initial_total_issuance = TotalIssuance::<Runtime>::get();

        initialize_block(block, None);

        for (lapse, origin, extrinsic) in extrinsics {
            if lapse > 0 {
                let prev_header = finalize_block(elapsed);

                // We update our state variables
                block += u32::from(lapse);
                weight = Weight::zero();
                elapsed = Duration::ZERO;

                // We start the next block
                initialize_block(block, Some(&prev_header));
            }

            weight.saturating_accrue(extrinsic.get_dispatch_info().call_weight);
            if weight.ref_time()
                >= RuntimeBlockWeights::get()
                    .get(DispatchClass::Normal)
                    .max_extrinsic
                    .unwrap()
                    .ref_time()
            {
                log::debug!("Skipping because of max weight {weight}");
                continue;
            }

            let origin: AccountId = accounts[origin as usize % accounts.len()]
                .clone()
                .account_id();

            log::debug!("origin: {origin:?}");
            log::debug!("call: {extrinsic:?}");

            let account = Account::<Runtime>::get(&origin);
            if account.data.free == 0 {
                continue;
            }

            let now = Instant::now();
            let res = extrinsic.dispatch(RuntimeOrigin::signed(origin));
            log::debug!("result: {res:?}");

            elapsed += now.elapsed();
        }

        finalize_block(elapsed);

        invariants_check::check_invariants(block, initial_total_issuance);
    });
}

/// Initializes a new block.
fn initialize_block(block: u32, prev_header: Option<&RelayHeader>) {
    log::debug!("initializing block: {block}");

    let pre_digest = Digest {
        logs: vec![DigestItem::PreRuntime(
            AURA_ENGINE_ID,
            Slot::from(u64::from(block)).encode(),
        )],
    };
    let parent_header = &Header::new(
        block,
        H256::default(),
        H256::default(),
        prev_header.map(Header::hash).unwrap_or_default(),
        pre_digest,
    );
    Executive::initialize_block(parent_header);

    // 2. Apply Timestamp
    Executive::apply_extrinsic(UncheckedExtrinsic::new_bare(RuntimeCall::Timestamp(
        pallet_timestamp::Call::set {
            now: u64::from(block) * SLOT_DURATION,
        },
    )))
    .unwrap()
    .unwrap();

    // 3.  Set up parachain validation data
    let parachain_validation_data = {
        let parent_head = HeadData(prev_header.unwrap_or(parent_header).encode());

        let sproof_builder = RelayStateSproofBuilder {
            para_id: ParachainInfo::get(),
            included_para_head: Some(parent_head.clone()),
            current_slot: cumulus_primitives_core::relay_chain::Slot::from(2 * u64::from(block)),
            ..Default::default()
        };

        let (relay_storage_root, proof) = sproof_builder.into_state_root_and_proof();

        cumulus_pallet_parachain_system::Call::set_validation_data {
            data: cumulus_primitives_parachain_inherent::ParachainInherentData {
                validation_data: cumulus_primitives_core::PersistedValidationData {
                    parent_head: Default::default(),
                    relay_parent_number: block,
                    relay_parent_storage_root: relay_storage_root,
                    max_pov_size: Default::default(),
                },
                relay_chain_state: proof,
                downward_messages: Default::default(),
                horizontal_messages: Default::default(),
            },
        }
    };

    Executive::apply_extrinsic(UncheckedExtrinsic::new_bare(RuntimeCall::ParachainSystem(
        parachain_validation_data,
    )))
    .unwrap()
    .unwrap();
}

/// Finalizes the current block and returns its header.
fn finalize_block(elapsed: Duration) -> RelayHeader {
    log::debug!("time spent: {elapsed:?}");

    assert!(elapsed.as_secs() <= 2, "block execution took too much time");

    log::debug!("finalizing block");
    Executive::finalize_block()
}

/// Recursively find call types within nested runtime calls.
fn recursively_find_call(call: RuntimeCall, matches_on: fn(&RuntimeCall) -> bool) -> bool {
    if let RuntimeCall::Utility(
        pallet_utility::Call::batch { calls }
        | pallet_utility::Call::force_batch { calls }
        | pallet_utility::Call::batch_all { calls },
    ) = call
    {
        for call in calls {
            if recursively_find_call(call.clone(), matches_on) {
                return true;
            }
        }
    } else if let RuntimeCall::Multisig(pallet_multisig::Call::as_multi_threshold_1 {
        call, ..
    })
    | RuntimeCall::Utility(pallet_utility::Call::as_derivative { call, .. })
    | RuntimeCall::Proxy(pallet_proxy::Call::proxy { call, .. })
    | RuntimeCall::Council(pallet_collective::Call::propose {
        proposal: call, ..
    }) = call
    {
        return recursively_find_call(*call, matches_on);
    } else if matches_on(&call) {
        return true;
    }
    false
}

/// Filters out slow calls to avoid fuzzer timeouts.
fn call_filter(call: &RuntimeCall) -> bool {
    // We filter out contracts call that will take too long because of fuzzer instrumentation
    matches!(
        &call,
        RuntimeCall::Contracts(
            pallet_contracts::Call::instantiate_with_code { .. }
                | pallet_contracts::Call::upload_code { .. }
                | pallet_contracts::Call::instantiate_with_code_old_weight { .. }
                | pallet_contracts::Call::migrate { .. }
        )
    )
}
