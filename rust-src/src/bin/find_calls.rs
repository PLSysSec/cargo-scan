/*
Parse a Rust source file and find all function calls, printing them to stdout
(one per line).
*/

use cargo_scan::effect::Effect;
use clap::Parser;
use std::fs::File;
use std::io::Read;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    filename: String,
}

fn main() {
    let args = Args::parse();

    // based on example at https://docs.rs/syn/latest/syn/struct.File.html
    let mut file = File::open(&args.filename).expect("Unable to open file");
    let mut src = String::new();
    file.read_to_string(&mut src).expect("Unable to read file");
    let syntax = syn::parse_file(&src).expect("Unable to parse file");
    println!("{:#?}", syntax);
}
