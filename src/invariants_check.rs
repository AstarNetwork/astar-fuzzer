use astar_primitives::Balance;
use astar_runtime::{AllPalletsWithSystem, Balances, Runtime};
use frame_support::traits::{TryState, TryStateSelect};
use frame_system::Account;
use pallet_balances::Holds;

/// Validates runtime invariants after every block finalization.
/// Use Runtime Hooks (integrity_test and try_state) that are run on critical state pallets
pub fn check_invariants(block: u32, _initial_total_issuance: Balance) {
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

    AllPalletsWithSystem::try_state(block, TryStateSelect::All).unwrap();
}
