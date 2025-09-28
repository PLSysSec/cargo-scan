use wasm_bindgen::prelude::*;

type Request = isize;
type Promise = isize;

use std::fs;
use std::io::Write;
use std::fs::File;

macro_rules! my_unsafe_fn {
    () => {
        unsafe fn inner_unsafe_fn() {
            println!("I am unsafe");
            let x: i32 = 5;
            // Never do this
            let y: *mut i32 = x as *mut i32;
            *y = 6; // segfault
        }
        unsafe {inner_unsafe_fn()};
    };
}

macro_rules! unsafe_block_ex {
    () => {
        fn inner_unsafe_block_ex() {
            println!("I have an unsafe block");
            let x: i32 = 5;
            // Never do this
            let y: *mut i32 = x as *mut i32;
            unsafe {
                *y = 6; // segfault
            }
        }
        inner_unsafe_block_ex();
    };
}

macro_rules! my_unsafe_ffi {
    () => {
        extern "C" {
            static MY_EXTERN_STATIC: i32;
            pub fn my_unsafe_c_ffi();
        }
        unsafe {
            my_unsafe_c_ffi();
            let ret = MY_EXTERN_STATIC;
        }
    };
}

macro_rules! unsafe_union_access {
    () => {
        union MyUnion {
            f1: i32,
            f2: bool,
        }

        fn test_union_access() {
            let mut my_union = MyUnion { f1: 5 };
            // assigning to a union field is a safe operation
            my_union.f1 = 10;
            unsafe {
                let ex = (MyUnion { f1: 5 }.f1, MyUnion { f2: false });
                if ex.1.f2 {
                    let union_vec = vec![my_union];
                    let arg = union_vec[0].f1 + 5;
                    println!("{:?}", MyUnion { f1: arg }.f1);
                }
            }
        }
        test_union_access();
    };
}

macro_rules! test_static_vars {
    () => {
        static mut MY_STATIC_VAR: i32 = 0;
        unsafe {
            MY_STATIC_VAR += 1;
        }
    };
}

macro_rules! test_logging {
    () => {
        use log::{error, info, warn, Record, Level, Metadata, LevelFilter};

        struct MyLogger;

        impl log::Log for MyLogger {
            fn enabled(&self, metadata: &Metadata) -> bool {
                metadata.level() <= Level::Info
            }

            fn log(&self, record: &Record) {
                if self.enabled(record.metadata()) {
                    println!("{} - {}", record.level(), record.args());
                }
            }
            fn flush(&self) {}
        }

        static MY_LOGGER: MyLogger = MyLogger;

        fn init_logging() {
            log::set_logger(&MY_LOGGER).unwrap();
            log::set_max_level(LevelFilter::Info);

            info!("Hello log");
            warn!("warning");
            error!("oops");
        }

        init_logging();
    };
}

macro_rules! file_operations {
    () => {
        fn perform_file_ops() {
            let mut file = File::create("foo.txt").unwrap();
            file.write_all(b"Hello, test").unwrap();

            let a = fs::read("Cargo.toml");
            match a {
                Ok(content) => println!("Read Cargo.toml successfully: {:?}", content),
                Err(e) => println!("Failed to read Cargo.toml: {:?}", e),
            }
        }

        perform_file_ops();
    };
}

#[macro_export]
macro_rules! simulate_ffi {
    () => {
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
    };
}

unsafe fn add_one(x: i32) -> i32 {
    x + 1
}

macro_rules! fn_ptr_creation {
    () => {
        fn test_ptr() {
            let f: unsafe fn(i32) -> i32 = add_one;
            unsafe {
                let _ = f(10);
            }
        }

        test_ptr();
    };
}

macro_rules! closure_creation {
    () => {
        fn test_closure() {
            let x = 100;
            let c = |y: i32| x + y;
            let _ = c(23);
        }

        test_closure();
    };
}



fn main() {
    my_unsafe_fn!();
    unsafe_block_ex!();
    my_unsafe_ffi!();
    unsafe_union_access!();
    test_static_vars!();
    test_logging!();
    file_operations!();
    simulate_ffi!();
    fn_ptr_creation!();
    closure_creation!();
}