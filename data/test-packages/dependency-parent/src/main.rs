pub fn internal_unsafe_deref() -> Option<u32> {
    let x: i32 = 5;
    let y: *mut i32 = x as *mut i32;
    unsafe {
        *y = 6;
    }
    Some(1)
}

fn main() {
    internal_unsafe_deref();
    dependency_ex::read_fn();
    dependency_ex::unsafe_deref();
}
