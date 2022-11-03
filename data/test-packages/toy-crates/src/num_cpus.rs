/*
    Toy version of the num_cpus crate.
*/

use libc;
use std::ffi::CStr;
use std::mem;
use std::ptr;

#[cfg(target_os = "macos")]
pub fn get_num_physical_cpus() -> usize {
    let mut cpus: i32 = 0;
    let mut cpus_size = mem::size_of_val(&cpus);

    let sysctl_name = CStr::from_bytes_with_nul(b"hw.physicalcpu\0").unwrap();

    unsafe {
        libc::sysctlbyname(
            sysctl_name.as_ptr(),
            &mut cpus as *mut _ as *mut _,
            &mut cpus_size as *mut _ as *mut _,
            ptr::null_mut(),
            0,
        );
    }
    cpus as usize
}

#[cfg(target_os = "macos")]
pub fn get_num_cpus() -> usize {
    const CONF_NAME: libc::c_int = libc::_SC_NPROCESSORS_CONF;
    let cpus = unsafe { libc::sysconf(CONF_NAME) };

    if cpus < 1 {
        1
    } else {
        cpus as usize
    }
}

#[cfg(target_os = "linux")]
pub fn get_num_physical_cpus() -> usize {
    unimplemented!()
}

#[cfg(target_os = "linux")]
pub fn get_num_cpus() -> usize {
    unimplemented!()
}
