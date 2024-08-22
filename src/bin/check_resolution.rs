/*
    This binary is intended for internal use.

    The main supported binaries are `--bin scan` and `--bin audit`.
    See README.md for usage instructions.
*/

#![allow(unused_variables)]

use std::path::PathBuf;

use anyhow::Result;
use cargo_scan::{
    effect::SrcLoc,
    ident::Ident,
    resolution::name_resolution::{Resolver, ResolverImpl},
};
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
    eprintln!("Warning: `--bin check_resolution` is intended for internal use. The primary supported binaries are `--bin scan` and `--bin audit`.");

    cargo_scan::util::init_logging();
    let args = Args::parse();

    let crate_name = cargo_scan::util::load_cargo_toml(&args.crate_path)?.crate_name;
    let mut filepath = std::path::PathBuf::from(&args.crate_path);
    filepath.push(&args.file);

    let resolver = Resolver::new(&args.crate_path)?;
    let file_resolver = ResolverImpl::new(&resolver, &filepath)?;

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
        match file_resolver.resolve_ident(s, i) {
            Ok(r) => println!("{:?} ===> {:?}", out, r.to_path().to_string()),
            Err(r) => println!("{:?}", r),
        }
    } else {
        match file_resolver.resolve_type(s, i) {
            Ok(r) => println!("{:?} ===> {:?}", out, r.to_string()),
            Err(r) => println!("{:?}", r),
        }
    }

    Ok(())
}
