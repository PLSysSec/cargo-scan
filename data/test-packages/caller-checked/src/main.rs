use core::ffi::CStr;
use core::ptr;

fn effect() {
    let _hw_physicalcpu = CStr::from_bytes_with_nul(b"hw.physicalcpu\0").unwrap();
    libc::sysctlbyname(
        hw_physicalcpu.as_ptr(),
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null_mut(),
        0,
    );
}

fn call1() {
    for _i in 0..10 {
        effect();
    }
}

fn nested_call2() {
    effect();
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
    println!("{}", a);
}
