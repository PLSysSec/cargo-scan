use cargo_scan::scanner;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    // ../path/to/my_rust_crate/src/my_mod/my_file.rs
    filepath: PathBuf,
}

fn main() {
    let args = Args::parse();

    let results = scanner::load_and_scan(&args.filepath);

    if !results.unsafe_blocks.is_empty() {
        println!("=== Unsafe blocks ===");
        for bl_decl in results.unsafe_blocks {
            println!("{:?}", bl_decl);
        }
    }

    if !results.unsafe_decls.is_empty() {
        println!("=== Unsafe fn declarations ===");
        for fn_decl in results.unsafe_decls {
            println!("{:?}", fn_decl);
        }
    }

    if !results.unsafe_traits.is_empty() {
        println!("=== Unsafe trait declarations ===");
        for tr_decl in results.unsafe_traits {
            println!("{:?}", tr_decl);
        }
    }

    if !results.unsafe_impls.is_empty() {
        println!("=== Unsafe trait impls ===");
        for impl_decl in results.unsafe_impls {
            println!("{:?}", impl_decl);
        }
    }

    if !results.ffi_calls.is_empty() {
        println!("=== FFI Calls ===");
        for ffi_call in results.ffi_calls {
            println!("{:?}", ffi_call);
        }
    }
}
