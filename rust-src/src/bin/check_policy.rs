/*
    Parse a Rust source file and check it against a policy file.

    Print out effects found that fail the policy, + whether or not
    the policy passes at the end.
*/

use cargo_scan::ident::{Path, Pattern};
use cargo_scan::policy::{Policy, PolicyLookup};
use cargo_scan::scanner;
use cargo_scan::util;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    // path/to/my_file.rs
    source: PathBuf,
    // path/to/my_policy.toml
    policy: PathBuf,
    // patterns of interest
    of_interest: PathBuf,
}

fn main() {
    let args = Args::parse();

    let of_interest: Vec<Pattern> =
        util::file_lines(&args.of_interest).map(Pattern::new_owned).collect();
    // println!("Of interest: {:?}", of_interest);

    let policy = Policy::from_file(&args.policy).unwrap();
    let mut lookup = PolicyLookup::from_policy(&policy);
    for pat in &of_interest {
        lookup.mark_of_interest(pat);
    }

    let mut errors = Vec::new();
    let results = scanner::load_and_scan(&args.source);
    for effect in results.effects {
        // println!("{}", effect.to_csv());
        let caller = Path::new(effect.caller_path());
        let callee = Path::new(effect.callee_path());
        // println!("{} -> {}", caller, callee);

        if !lookup.check_edge(&caller, &callee, &mut errors) {
            println!("{}", effect.to_csv());
        }
    }

    for err in &errors {
        eprintln!("policy error: {}", err);
    }

    if errors.is_empty() {
        eprintln!("policy passed");
    } else {
        eprintln!("policy failed with {} errors", errors.len());
    }
}
