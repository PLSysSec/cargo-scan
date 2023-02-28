use caller_checked::sub;
use core::ffi::CStr;
use core::ptr;

fn local_effect() {
    let hw_physicalcpu =
		CStr::from_bytes_with_nul(b"hw.physicalcpu\0").unwrap();
    unsafe {
        libc::sysctl(
            hw_physicalcpu.as_ptr() as *mut i32,
            14,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
        );
    }
}

fn local_call1() {
    local_effect();
}

fn call1() {
    for _i in 0..10 {
        sub::effect();
    }
}

fn nested_call2() {
    sub::effect();
}

fn call2() -> i32 {
    println!("into call2");
    nested_call2();
    1
}

fn main() {
    println!("Hello, world!");
    call1();
    let a = call2();
    local_call1();
    println!("{}", a);
}
