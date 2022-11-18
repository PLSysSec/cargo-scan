/*
    Minimal, platform-specific version of the num_cpus crate
    (Tested on MacOS Monterey M1)
*/

use std::ffi::CStr;
use std::mem;
use std::ptr;

const CONF_NAME: libc::c_int = 57;

fn get_num_cpus() -> usize {
    let cpus = unsafe { libc::sysconf(CONF_NAME) };
    if cpus < 1 {
        1
    } else {
        cpus as usize
    }
}

fn get_num_physical_cpus() -> usize {
    let mut cpus: i32 = 0;
    let mut cpus_size = mem::size_of_val(&cpus);

    let sysctl_name = CStr::from_bytes_with_nul(b"hw.physicalcpu\0").unwrap();

    unsafe {
        let return_code = libc::sysctlbyname(
            sysctl_name.as_ptr(),
            &mut cpus as *mut _ as *mut _,
            &mut cpus_size as *mut _ as *mut _,
            ptr::null_mut(),
            0,
        );
        assert_eq!(return_code, 0);
    }
    cpus as usize
}

fn main() {
    println!("Num CPUs: {}", get_num_cpus());
    println!("Num physical CPUs: {}", get_num_physical_cpus());
}
