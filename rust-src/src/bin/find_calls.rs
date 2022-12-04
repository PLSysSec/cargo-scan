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
struct Scanner<'a> {
    results: Vec<Effect>,
    scope_mods: Vec<&'a syn::Ident>,
}

impl<'a> Scanner<'a> {
    fn new() -> Self {
        let results = Vec::new();
        let scope_mods = Vec::new();
        Self { results, scope_mods }
    }
    fn scan_file(&mut self, f: &'a syn::File) {
        // scan the file and return a list of all calls in it
        for i in &f.items {
            self.scan_item(i);
        }
    }
    fn scan_item(&mut self, i: &'a syn::Item) {
        match i {
            syn::Item::Mod(m) => self.scan_mod(m),
            syn::Item::Use(u) => self.scan_use(u),
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
    fn scan_mod(&mut self, m: &'a syn::ItemMod) {
        if let Some((_, items)) = &m.content {
            self.scope_mods.push(&m.ident);
            for i in items {
                self.scan_item(i);
            }
            self.scope_mods.pop();
        }
    }
    fn scan_use(&mut self, _u: &'a syn::ItemUse) {
        todo!()
    }
    fn scan_fn(&mut self, _f: &'a syn::ItemFn) {
        todo!()
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
