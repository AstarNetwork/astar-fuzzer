mod invariants_check;
mod process_input;
mod utils;

use crate::process_input::process_input;

/// Entry point for the fuzzer, runs fuzzing on random input data.
fn main() {
    let genesis = utils::generate_genesis();
    let accounts = utils::accounts();
    ziggy::fuzz!(|data: &[u8]| {
        process_input(&accounts, &genesis, data);
    });
}
