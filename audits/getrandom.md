# [getrandom](https://docs.rs/getrandom/latest/getrandom/)

Audited by: Caleb Stanford

Date: 2022-10-12

16th most downloaded crate.
Contrast with `rand`: OS-based rather than software-based random number
generation.
Exposes a single function: `getrandom`

In the top 100 crates, `memchr` and `getrandom` are the only
crates that had no stdlib dangerous imports, and were only
flagged after including third party imports (in both cases, `libc`).

## List of imports (15)

```
src, 3ds.rs, crate::util_libc::sys_fill_exact
src, bsd_arandom.rs, crate::util_libc::sys_fill_exact
src, dragonfly.rs, crate::util_libc::Weak
src, dragonfly.rs, crate::util_libc::sys_fill_exact
src, linux_android.rs, crate::util_libc::last_os_error
src, linux_android.rs, crate::util_libc::sys_fill_exact
src, macos.rs, crate::util_libc::Weak
src, macos.rs, crate::util_libc::last_os_error
src, openbsd.rs, crate::util_libc::last_os_error
src, solaris_illumos.rs, crate::util_libc::Weak
src, solaris_illumos.rs, crate::util_libc::sys_fill_exact
src, use_file.rs, crate::util_libc::open_readonly
src, use_file.rs, crate::util_libc::sys_fill_exact
src, util_libc.rs, libc::c_void
src, vxworks.rs, crate::util_libc::last_os_error
```

## Analysis

All the imports are `libc`.
The `util_libc` imports are actually wrappers around `libc` functions.
The script misses the actual `libc` imports due to limited pattern matching.

The following libc imports are used:
- geterrno
- dlsym (!)
- open

## Security summary

1. Security risks

Likely minimal.
However, if the `custom` CFG option is used, there's a more serious
security risk as this provides the `register_custom_getrandom`
macro which can link in arbitrary code to be used with getrandom.

`util_libc` contains a very dangerous function `Weak::ptr` which calls
`dlsym`. But `util_libc` is private, and only appears to use this function
on fixed (static string) inputs: `GETRANDOM` and `GETENTROPY`.

2. Permissions

Read-only file system access to a fixed set of files;
ability to run a fixed set of syscalls

3. Transitive risk

None

4. Automation feasibility

- Spec: project-independent
- Static analysis: feasible
- Dynamic enforcement overhead: unsure
