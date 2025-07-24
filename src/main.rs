use astar_primitives::genesis::GenesisAccount;
use astar_primitives::parachain::ASTAR_ID;
use astar_primitives::{AccountId, Balance};
use astar_runtime::{
    genesis_config, AllPalletsWithSystem, Balances, Executive, ParachainInfo, Runtime, RuntimeCall,
    RuntimeGenesisConfig, RuntimeOrigin, UncheckedExtrinsic, SLOT_DURATION,
};
use codec::{DecodeLimit, Encode};
use cumulus_primitives_core::relay_chain::HeadData;
use cumulus_primitives_core::relay_chain::Header as RelayHeader;
use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
use frame_support::dispatch::GetDispatchInfo;
use frame_support::traits::{IntegrityTest, TryState, TryStateSelect};
use frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND;
use frame_support::weights::Weight;
use frame_system::Account;
use pallet_balances::{Holds, TotalIssuance};
use pallet_dapp_staking::Ledger;
use sp_consensus_aura::{Slot, AURA_ENGINE_ID};
use sp_core::sr25519::Public;
use sp_core::H256;
use sp_runtime::traits::{Dispatchable, Get, Header};
use sp_runtime::{BuildStorage, Digest, DigestItem, Storage};
use sp_state_machine::BasicExternalities;
use std::iter;
use std::time::{Duration, Instant};

fn main() {
    let alice = GenesisAccount::<Public>::from_seed("Alice");
    let bob = GenesisAccount::<Public>::from_seed("Bob");
    let charlie = GenesisAccount::<Public>::from_seed("Charlie");
    let dave = GenesisAccount::<Public>::from_seed("Dave");
    let eve = GenesisAccount::<Public>::from_seed("Eve");

    let accounts = vec![&alice, &bob, &charlie, &dave, &eve];

    let genesis = generate_genesis();

    ziggy::fuzz!(|data: &[u8]| {
        process_input(&accounts, &genesis, data);
    });
}

fn generate_genesis() -> Storage {
    let genesis_json = genesis_config::default_config(ASTAR_ID);
    let genesis_config: RuntimeGenesisConfig = serde_json::from_value(genesis_json).unwrap();
    genesis_config.build_storage().unwrap()
}

fn process_input(accounts: &[&GenesisAccount<Public>], genesis: &Storage, data: &[u8]) {
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
            if weight.ref_time() >= 2 * WEIGHT_REF_TIME_PER_SECOND {
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

        check_invariants(block, initial_total_issuance);
    });
}

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

fn finalize_block(elapsed: Duration) -> RelayHeader {
    log::debug!("time spent: {elapsed:?}");

    assert!(elapsed.as_secs() <= 2, "block execution took too much time");

    log::debug!("finalizing block");
    Executive::finalize_block()
}

fn check_invariants(block: u32, _initial_total_issuance: Balance) {
    for (account, info) in Account::<Runtime>::iter() {
        let consumers = info.consumers;
        let providers = info.providers;
        assert!(!(consumers > 0 && providers == 0), "Invalid c/p state");
        let max_lock: Balance = Balances::locks(&account)
            .iter()
            .map(|l| l.amount)
            .max()
            .unwrap_or_default();
        assert_eq!(
            max_lock, info.data.frozen,
            "Max lock should be equal to frozen balance"
        );
        let sum_holds: Balance = Holds::<Runtime>::get(&account)
            .iter()
            .map(|l| l.amount)
            .sum();
        assert!(
            sum_holds <= info.data.reserved,
            "Sum of all holds ({sum_holds}) should be less than or equal to reserved balance {}",
            info.data.reserved
        );
    }

    check_dapp_staking_invariants();

    AllPalletsWithSystem::integrity_test();
    AllPalletsWithSystem::try_state(block, TryStateSelect::All).unwrap();
}

fn check_dapp_staking_invariants() {
    use pallet_dapp_staking::{CurrentEraInfo, StakerInfo};

    // Check that total staked doesn't exceed total issuance
    let current_era_info = CurrentEraInfo::<Runtime>::get();
    let total_staked = current_era_info.total_staked_amount();
    let total_issuance = TotalIssuance::<Runtime>::get();

    assert!(total_staked <= total_issuance);

    // Verify staker info consistency
    let mut counted_individual_stakes = 0;
    for (staker, _smart_contract, staker_info) in StakerInfo::<Runtime>::iter() {
        counted_individual_stakes += staker_info.total_staked_amount();
        assert!(Ledger::<Runtime>::contains_key(&staker));
    }

    assert!(counted_individual_stakes <= total_staked);
}

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
