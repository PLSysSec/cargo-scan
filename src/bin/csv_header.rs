/*
    Print out the Effect CSV header.
*/

use cargo_scan::effect::EffectInstance;

fn main() {
    cargo_scan::util::init_logging();
    println!("{}", EffectInstance::csv_header());
}
