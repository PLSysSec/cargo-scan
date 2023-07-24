/*
    Example derived from reqwest
    https://github.com/PLSysSec/cargo-scan/issues/27

    Oddly shows that when using the #[wasm_bindgen] attribute it's possible
    to call FFI functions without the unsafe keyword.
    This file seems to build without warnings.
*/

use wasm_bindgen::prelude::*;

type Request = isize;
type Promise = isize;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen]
    fn fetch_with_request(input: Request) -> Promise;
}

// calling an FFI function without unsafe!
pub fn call_fn() -> String {
    let result = fetch_with_request(32);
    format!("{}", result)
}

fn main() {
    println!("{}", call_fn());
}
