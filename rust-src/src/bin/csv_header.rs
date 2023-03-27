/*
    Print out the Effect CSV header.
*/

use cargo_scan::effect::EffectInstance;

fn main() {
    println!("{}", EffectInstance::csv_header());
}
