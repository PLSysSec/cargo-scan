# [num_cpus](https://docs.rs/num_cpus/latest/num_cpus/)

Audited by: Caleb Stanford
Date: 2022-10-05

Top 100 most downloaded crates.

## List of imports (3)

```
linux.rs, std::fs::File
linux.rs, std::path::Path
linux.rs, std::path::PathBuf
```

## Analysis

This is an interesting crate, roughly similar in nature to the
rustc version crates, for getting the number of CPUs of the current system.

Besides the above imports, there are other side-effectful statements
using `libc` system call stuff for Windows/MacOS code as well as the linux
file. The file system access is there, too (it reads `/proc/cpuinfo`);
there's a lot of implementation magic here to get what the crate needs.

## Security summary

1. Security risks

Minimal

2. Permissions

Potentially needs read-only filesystem access to a few specific locations
Needs system access to call specific system calls (`libc::sysconf` and
`libc::sysctl` in particular).

3. Transitive risk

No

4. Automation feasibility

- Spec: platform-independent. Probably feasible but difficult.
- Static analysis: feasible
- Dynamic enforcement overhead: acceptable
