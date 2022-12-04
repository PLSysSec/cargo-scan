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

/// Stateful object to scan Rust source code for effects (fn calls of interest)
struct Scanner {
    results: Vec<Effect>,
}

impl Scanner {
    fn new() -> Self {
        let results = Vec::new();
        Self { results }
    }
    fn scan_use(&mut self, _u: &syn::ItemUse) {
        todo!()
    }
    fn scan_mod(&mut self, _m: &syn::ItemMod) {
        todo!()
    }
    fn scan_fn(&mut self, _f: &syn::ItemFn) {
        todo!()
    }
    fn scan_file(&mut self, f: &syn::File) {
        // scan the file and return a list of all calls in it
        for item in &f.items {
            match item {
                syn::Item::Use(u) => self.scan_use(u),
                syn::Item::Mod(m) => self.scan_mod(m),
                syn::Item::Fn(fun) => self.scan_fn(fun),
                _ => (),
                // For all syntax elements see
                // https://docs.rs/syn/latest/syn/enum.Item.html
                // Potentially interesting:
                // Trait(t) -- default impls are an issue here
                // ForeignMod(ItemForeignMod) -- extern items like extern "C" { ... }
                // Macro, Macro2 -- macro invocations and declarations
                // Const(ItemConst), Static(ItemStatic) -- for information flow
            }
        }
    }
}

fn main() {
    let args = Args::parse();

    // based on example at https://docs.rs/syn/latest/syn/struct.File.html
    let mut file = File::open(&args.filename).expect("Unable to open file");
    let mut src = String::new();
    file.read_to_string(&mut src).expect("Unable to read file");

    let syntax_tree = syn::parse_file(&src).expect("Unable to parse file");
    let mut scanner = Scanner::new();
    scanner.scan_file(&syntax_tree);

    for effect in scanner.results {
        println!("{}", effect.to_csv());
    }
}
