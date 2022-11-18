use core::ffi::CStr;
use core::ptr;

fn main() {
    unsafe {
        libc::sysconf(57);
        let hw_physicalcpu = CStr::from_bytes_with_nul(b"hw.physicalcpu\0").unwrap();
        libc::sysctlbyname(
            hw_physicalcpu.as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
        );
    }
}
