pub mod wasm_bindgen_ex;

unsafe fn my_unsafe_fn() {
    println!("I am unsafe");
    let x: i32 = 5;
    // Never do this
    let y: *mut i32 = x as *mut i32;
    *y = 6; // segfault
}

fn unsafe_block_ex() {
    println!("I have an unsafe block");
    let x: i32 = 5;
    // Never do this
    let y: *mut i32 = x as *mut i32;
    unsafe {
        *y = 6; // segfault
    }
}

extern "C" {
    static MY_EXTERN_STATIC: i32;
    pub fn my_unsafe_c_ffi();
}

union MyUnion {
    f1: i32,
    f2: bool,
}

fn get_my_union(arg: i32) -> MyUnion {
    MyUnion{f1: arg}
}
pub struct MyEx (pub i32, MyUnion);

static mut MY_STATIC_VAR: i32 = 0;

fn main() {
    println!("Hello, world!");
    unsafe {
        my_unsafe_fn();
    }
    println!("FFI example");
    unsafe {
        my_unsafe_c_ffi();
    }

    // examples of union field accesses
    let mut my_union = MyUnion{f1: 5};
    // assigning to a union field is a safe operation
    my_union.f1 = 10;
    unsafe {       
        let ex = MyEx(MyUnion{f1: 5}.f1, MyUnion{f2: false});
        if ex.1.f2 {
            let union_vec= vec![my_union]; 
            let arg = union_vec[0].f1 + 5;
            println!("{:?}", get_my_union(arg).f1);
        }       
    }

    // accessing static mutable/extern variables
    unsafe { 
        let ret = MY_EXTERN_STATIC;
        MY_STATIC_VAR += 1;
    }
}
