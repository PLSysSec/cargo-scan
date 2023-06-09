#![allow(unused_variables)]

use std::path::PathBuf;

use anyhow::Result;
use cargo_scan::{effect::SrcLoc, ident::Ident, name_resolution::Resolver};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to crate directory; should contain a 'src' directory and a Cargo.toml file
    crate_path: PathBuf,
    line: usize,
    col: usize,
    name: String,

    #[arg(short = 't', long = "type")]
    resolve_type: bool,
    #[arg(short, long, default_value = "src/main.rs")]
    file: PathBuf,
}

pub fn main() -> Result<()> {
    cargo_scan::util::init_logging();
    let args = Args::parse();

    let res = Resolver::new(&args.crate_path).unwrap();
    let mut filepath = std::path::PathBuf::from(&args.crate_path);
    filepath.push(&args.file);

    let s = SrcLoc::new(filepath.as_path(), args.line, args.col, args.line, args.col);
    let i = Ident::new(&args.name);

    let mut out = match args.resolve_type {
        true => String::from("Canonical type for '"),
        _ => String::from("Canonical path for '"),
    };
    out.push_str(&args.name);
    out.push_str("' on {ln ");
    out.push_str(&args.line.to_string());
    out.push_str(",col ");
    out.push_str(&args.col.to_string());
    out.push('}');

    if !args.resolve_type {
        match res.resolve_ident(s, i) {
            Ok(r) => println!("{:?} ===> {:?}", out, r.to_path().to_string()),
            Err(r) => println!("{:?}", r),
        }
    } else {
        match res.resolve_type(s, i) {
            Ok(r) => println!("{:?} ===> {:?}", out, r.to_string()),
            Err(r) => println!("{:?}", r),
        }
    }

    Ok(())
}
