/*
    Parse a Rust source file and check it against a policy file.
*/

use cargo_scan::policy::{IdentPath, Policy, PolicyLookup};
use cargo_scan::scanner;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    // path/to/my_file.rs
    source: PathBuf,
    // path/to/my_policy.toml
    policy: PathBuf,
}

fn main() {
    let args = Args::parse();

    let policy = Policy::from_file(&args.policy).unwrap();
    // TODO: add patterns
    let lookup = PolicyLookup::from_policy(&policy);

    let mut errors = Vec::new();
    let results = scanner::load_and_scan(&args.source);
    for effect in results.effects {
        let caller = IdentPath::new(effect.caller_path());
        let callee = IdentPath::new(effect.callee_path());
        lookup.check_edge(&caller, &callee, &mut errors);
    }

    for err in &errors {
        println!("policy error: {}", err);
    }
}
