# [parking_lot_core](https://docs.rs/parking_lot_core/latest/parking_lot_core/)

Audited by: Caleb Stanford

Date: 2022-10-12

27th most downloaded crate

## List of imports (9)

```
src/thread_parker, linux.rs, libc
src/thread_parker, sgx.rs, std::os::fortanix_sgx::
src/thread_parker, sgx.rs, std::os::fortanix_sgx::thread::current as current_tcs
src/thread_parker, sgx.rs, std::os::fortanix_sgx::usercalls::
src/thread_parker, sgx.rs, std::os::fortanix_sgx::usercalls::raw::EV_UNPARK
src/thread_parker, sgx.rs, std::os::fortanix_sgx::usercalls::raw::Tcs
src/thread_parker, sgx.rs, std::os::fortanix_sgx::usercalls::raw::WAIT_INDEFINITE
src/thread_parker, sgx.rs, std::os::fortanix_sgx::usercalls::self
src/thread_parker, unix.rs, libc
```

## Analysis

For the unix and linux files, uses a bunch of libc pthreads stuff:
`pthread_mutex_unlock`, `pthread_mutex_wait`, etc.

Also uses `libc::gettimeofday` and `libc::clock_gettime`.

## Security summary

1. Security risks

None

2. Permissions

Needs to access the system clock and create threads.

3. Transitive risk

None

4. Automation feasibility

- Spec: project-independent
- Static analysis: feasible
- Dynamic enforcement overhead: likely not acceptable
