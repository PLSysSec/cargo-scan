use macro_test::{simulate_dangerous};


use wasm_bindgen::prelude::*;

type Request = isize;
type Promise = isize;

#[macro_export]
macro_rules! simulate_dangerous {
    (unsafe_memory) => {{
        unsafe {
            println!("Simulating unsafe memory access...");
            let x: i32 = 5;
            // Cast a value to a mutable raw pointer (this is highly unsafe and should never be done)
            let y: *mut i32 = x as *mut i32;
            *y = 6; // This causes undefined behavior (e.g., a segmentation fault)
        }
    }};
}

#[macro_export]
macro_rules! simulate_ffi {
    () => {{
        // Define a mock FFI function
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
        // Execute the function
        call_fn()
    }};

}fn main() {
    simulate_dangerous!(unsafe_memory);
    simulate_ffi!();
}