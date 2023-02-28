use core::ffi::CStr;
use core::ptr;

pub fn effect() {
    unsafe {
        libc::sysconf(57);
        let hw_physicalcpu = CStr::from_bytes_with_nul(b"hw.physicalcpu\0").unwrap();
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

