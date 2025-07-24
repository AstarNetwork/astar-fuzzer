use astar_primitives::evm::EVM_REVERT_CODE;
use astar_primitives::genesis::GenesisAccount;
use astar_primitives::oracle::CurrencyAmount;
use astar_primitives::{AccountId, Balance};
use astar_runtime::{
    AllPalletsWithSystem, Balances, CommunityTreasuryPalletId, Executive, ParachainInfo,
    Precompiles, Runtime, RuntimeCall, RuntimeOrigin, TreasuryPalletId, UncheckedExtrinsic,
    SLOT_DURATION,
};
use codec::{DecodeLimit, Encode};
use cumulus_primitives_core::relay_chain::HeadData;
use cumulus_primitives_core::relay_chain::Header as RelayHeader;
use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
use frame_support::dispatch::GetDispatchInfo;
use frame_support::traits::IntegrityTest;
use frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND;
use frame_support::weights::Weight;
use frame_system::Account;
use pallet_balances::{Holds, TotalIssuance};
use pallet_dapp_staking::Ledger;
use sp_consensus_aura::{Slot, AURA_ENGINE_ID};
use sp_core::sr25519::Public;
use sp_core::H256;
use sp_runtime::traits::{AccountIdConversion, Dispatchable, Get, Header};
use sp_runtime::{Digest, DigestItem, Storage};
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

    let genesis = generate_genesis(&accounts);

    ziggy::fuzz!(|data: &[u8]| {
        process_input(&accounts, &genesis, data);
    });
}

fn generate_genesis(accounts: &Vec<&GenesisAccount<Public>>) -> Storage {
    use astar_primitives::*;
    use astar_runtime::{
        AuraConfig, BalancesConfig, CollatorSelectionConfig, CommunityCouncilMembershipConfig,
        CouncilMembershipConfig, DappStakingConfig, EVMConfig, OracleMembershipConfig,
        ParachainInfoConfig, PriceAggregatorConfig, RuntimeGenesisConfig, SessionConfig,
        SessionKeys, SudoConfig, TechnicalCommitteeMembershipConfig, VestingConfig,
    };
    use pallet_dapp_staking::TierThreshold;
    use sp_runtime::BuildStorage;
    use sp_runtime::{Perbill, Permill};

    const ASTR: Balance = 1_000_000_000_000_000_000;

    let authorities = [&accounts[0], &accounts[1]];
    let account_ids = accounts.iter().map(|x| x.account_id()).collect::<Vec<_>>();

    let balances = account_ids
        .iter()
        .chain(
            [
                TreasuryPalletId::get().into_account_truncating(),
                CommunityTreasuryPalletId::get().into_account_truncating(),
            ]
            .iter(),
        )
        .map(|x| (x.clone(), 1_000_000_000 * ASTR))
        .collect::<Vec<_>>();

    let config = RuntimeGenesisConfig {
        system: Default::default(),
        sudo: SudoConfig {
            key: Some(accounts[0].account_id()),
        },
        parachain_info: ParachainInfoConfig {
            parachain_id: 2006u32.into(),
            ..Default::default()
        },
        balances: BalancesConfig { balances },
        vesting: VestingConfig { vesting: vec![] },
        session: SessionConfig {
            keys: authorities
                .iter()
                .map(|x| {
                    (
                        x.account_id(),
                        x.account_id(),
                        SessionKeys {
                            aura: x.pub_key().into(),
                        },
                    )
                })
                .collect::<Vec<_>>(),
            ..Default::default()
        },
        aura: AuraConfig {
            authorities: vec![],
        },
        aura_ext: Default::default(),
        collator_selection: CollatorSelectionConfig {
            desired_candidates: 32,
            candidacy_bond: 3_200_000 * ASTR,
            invulnerables: authorities
                .iter()
                .map(|x| x.account_id())
                .collect::<Vec<_>>(),
        },
        evm: EVMConfig {
            // We need _some_ code inserted at the precompile address so that
            // the evm will actually call the address.
            accounts: Precompiles::used_addresses_h160()
                .map(|addr| {
                    (
                        addr,
                        fp_evm::GenesisAccount {
                            nonce: Default::default(),
                            balance: Default::default(),
                            storage: Default::default(),
                            code: EVM_REVERT_CODE.into(),
                        },
                    )
                })
                .collect(),
            ..Default::default()
        },
        ethereum: Default::default(),
        polkadot_xcm: Default::default(),
        assets: Default::default(),
        parachain_system: Default::default(),
        transaction_payment: Default::default(),
        dapp_staking: DappStakingConfig {
            reward_portion: vec![
                Permill::from_percent(40),
                Permill::from_percent(30),
                Permill::from_percent(20),
                Permill::from_percent(10),
            ],
            slot_distribution: vec![
                Permill::from_percent(10),
                Permill::from_percent(20),
                Permill::from_percent(30),
                Permill::from_percent(40),
            ],
            tier_thresholds: vec![
                TierThreshold::DynamicPercentage {
                    percentage: Perbill::from_parts(35_700_000), // 3.57%
                    minimum_required_percentage: Perbill::from_parts(23_800_000), // 2.38%
                    maximum_possible_percentage: Perbill::from_percent(100),
                },
                TierThreshold::DynamicPercentage {
                    percentage: Perbill::from_parts(8_900_000), // 0.89%
                    minimum_required_percentage: Perbill::from_parts(6_000_000), // 0.6%
                    maximum_possible_percentage: Perbill::from_percent(100),
                },
                TierThreshold::DynamicPercentage {
                    percentage: Perbill::from_parts(2_380_000), // 0.238%
                    minimum_required_percentage: Perbill::from_parts(1_790_000), // 0.179%
                    maximum_possible_percentage: Perbill::from_percent(100),
                },
                TierThreshold::FixedPercentage {
                    required_percentage: Perbill::from_parts(600_000), // 0.06%
                },
            ],
            slots_per_tier: vec![10, 20, 30, 40],
            safeguard: Some(false),
            ..Default::default()
        },
        inflation: Default::default(),
        oracle_membership: OracleMembershipConfig {
            members: vec![accounts[0].account_id(), accounts[1].account_id()]
                .try_into()
                .expect("Assumption is that at least two members will be allowed."),
            ..Default::default()
        },
        price_aggregator: PriceAggregatorConfig {
            circular_buffer: vec![CurrencyAmount::from_rational(5, 10)]
                .try_into()
                .expect("Must work since buffer should have at least a single value."),
        },
        council_membership: CouncilMembershipConfig {
            members: account_ids
                .clone()
                .try_into()
                .expect("Should support at least 5 members."),
            phantom: Default::default(),
        },
        technical_committee_membership: TechnicalCommitteeMembershipConfig {
            members: account_ids[..3]
                .to_vec()
                .try_into()
                .expect("Should support at least 3 members."),
            phantom: Default::default(),
        },
        community_council_membership: CommunityCouncilMembershipConfig {
            members: account_ids
                .try_into()
                .expect("Should support at least 5 members."),
            phantom: Default::default(),
        },
        council: Default::default(),
        technical_committee: Default::default(),
        community_council: Default::default(),
        democracy: Default::default(),
        treasury: Default::default(),
        community_treasury: Default::default(),
    };

    config
        .build_storage()
        .expect("Genesis config should build successfully")
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
                #[cfg(not(feature = "fuzzing"))]
                println!("Skipping because of max weight {weight}");
                continue;
            }

            let origin: AccountId = accounts[origin as usize % accounts.len()]
                .clone()
                .account_id();

            #[cfg(not(feature = "fuzzing"))]
            println!("\n    origin:     {origin:?}");
            #[cfg(not(feature = "fuzzing"))]
            println!("    call:       {extrinsic:?}");

            let account = Account::<Runtime>::get(&origin);
            if account.data.free == 0 {
                continue;
            }

            let now = Instant::now();
            #[allow(unused_variables)]
            let res = extrinsic.dispatch(RuntimeOrigin::signed(origin));

            #[cfg(not(feature = "fuzzing"))]
            println!("    result:     {res:?}");

            elapsed += now.elapsed();
        }

        finalize_block(elapsed);

        check_invariants(block, initial_total_issuance);
    });
}

fn initialize_block(block: u32, prev_header: Option<&RelayHeader>) {
    #[cfg(not(feature = "fuzzing"))]
    println!("\ninitializing block {block}");

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
    #[cfg(not(feature = "fuzzing"))]
    println!("\n  time spent: {elapsed:?}");

    assert!(elapsed.as_secs() <= 2, "block execution took too much time");

    #[cfg(not(feature = "fuzzing"))]
    println!("finalizing block");
    Executive::finalize_block()
}

fn check_invariants(_block: u32, _initial_total_issuance: Balance) {
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

    AllPalletsWithSystem::integrity_test();
    check_dapp_staking_invariants();
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
