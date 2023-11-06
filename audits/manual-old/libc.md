# [libc](https://docs.rs/libc/0.2.134/libc/)

Audited by: Caleb Stanford

Date: 2022-10-07

libc is the 4th most downloaded crate, with 136878114 all-time downloads.

libc consists almost entirely of
(1) type definitions, and
(2) external FFI calls to C code.
All the FFI calls are inherent `unsafe fn`s in Rust's type system.
Most of the functions are effectful and access the file system, network,
or OS in some way, but as they are C code and not Rust,
our tool doesn't flag any dangerous imports.

libc is used widely as a low-level API by crates that need primitives
to interface with the system directly. It is a dependency for lots of
other important crates like mio and tokio.

## List of imports (0)

None (all dangerous behavior is inside unsafe C code)

## Analysis

For simplicity, I looked only at the Unix implementation.
The top-level library imports two public modules, `fixed_width_ints`
which just contains type defs, and `unix` which contains the important stuff.
This also contains some submodules.

Rough selection of some of the important stuff:
- File system ops: e.g. `fopen`, `fflush`, `fclose`, `printf`, `fprintf`,
  `mkdir`, `fchmod`, `chown`
- Mem operations: e.g. `malloc`, `free`, `memcpy`, `mmap`
- Termination: `exit`
- Config/environment: `system`, `getenv`
- Network: e.g. `socket`, `connect`, `listen`, `send`, `recv`
- Direct execution: `execv`, `execl`, etc.
- Process management: `waitpid`, `kill`
- Threads: all the `pthread` APIs
- Time: `time`

There's also a submodule for additional linux functions:
`clock_gettime`, `fstatfs`, `umount`, `sysinfo`, `mount`, `sendfile`,
`regexec`, `timer_create`, etc.

### Safety

Unlike crates like `safe-libc` or `nix`, `libc` doesn't contain
any safe abstractions over unsafe code; it's just the pure Rust
bindings and nothing else.

## Security summary

1. Security risks

Too many to enumerate

2. Permissions

Perhaps there is work somewhere on annotating individual libc functions
and syscalls with their permissions; there's a lot to enumerate though.
In the crate as a whole probably every single imaginable permission
is needed somewhere, except that it shouldn't break OS level
security assumptions (e.g. access root-owned files).

3. Transitive risk

`libc` is a textbook transitive risk case.
It's better analyzed and audited not in the crate itself, but at individual
sites where its functions are used in client crates like `mio`.
Updates to `libc` and the C code it calls into should be audited carefully.

4. Automation feasibility

- Spec: project-dependent; infeasible
- Static analysis: infeasible
- Dynamic enforcement overhead: project-dependent
