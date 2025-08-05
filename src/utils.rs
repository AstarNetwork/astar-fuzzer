use astar_primitives::genesis::GenesisAccount;
use astar_primitives::parachain::ASTAR_ID;
use astar_runtime::{genesis_config, RuntimeGenesisConfig};
use sp_core::sr25519::Public;
use sp_core::storage::Storage;
use sp_runtime::BuildStorage;

/// Creates genesis storage state.
pub fn generate_genesis() -> Storage {
    let genesis_json = genesis_config::default_config(ASTAR_ID);
    let genesis_config: RuntimeGenesisConfig = serde_json::from_value(genesis_json).unwrap();
    genesis_config.build_storage().unwrap()
}

/// Returns test accounts for fuzzing.
pub fn accounts() -> Vec<GenesisAccount<Public>> {
    let alice = GenesisAccount::<Public>::from_seed("Alice");
    let bob = GenesisAccount::<Public>::from_seed("Bob");
    let charlie = GenesisAccount::<Public>::from_seed("Charlie");
    let dave = GenesisAccount::<Public>::from_seed("Dave");
    let eve = GenesisAccount::<Public>::from_seed("Eve");

    vec![alice, bob, charlie, dave, eve]
}
