
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


fn main() {
    println!("Hello, world!");
    unsafe {
        my_unsafe_fn();
    }
}
