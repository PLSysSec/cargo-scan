# [cc](https://docs.rs/cc/latest/cc/)

Audited by: Caleb Stanford

Date: 2022-10-12

The 26th most downloaded crate.
Used only as a build dependency --
used to compile and link C code in `build.rs`.

## List of imports (18)

```
src, com.rs, std::os::windows::ffi::OsStrExt
src, com.rs, std::os::windows::ffi::OsStringExt
src, lib.rs, std::env
src, lib.rs, std::fs
src, lib.rs, std::path::Component
src, lib.rs, std::path::Path
src, lib.rs, std::path::PathBuf
src, lib.rs, std::process::Child
src, lib.rs, std::process::Command
src, lib.rs, std::process::Stdio
src, registry.rs, std::os::raw
src, registry.rs, std::os::windows::prelude::*
src, vs_instances.rs, std::path::PathBuf
src, winapi.rs, std::os::raw
src, windows_registry.rs, std::process::Command
src/bin, gcc-shim.rs, std::env
src/bin, gcc-shim.rs, std::fs::File
src/bin, gcc-shim.rs, std::path::PathBuf
```

## Security summary

1. Security risks

Code injection / arbitrary code execution at build time;
and potentially linking with dangerous C code.

2. Permissions

File system access; command execution; environment variables; etc.
Needs these permissions only at build time.

3. Transitive risk

Uncertain

4. Automation feasibility

- Spec: mostly project-independent
- Static analysis: possibly feasible
- Dynamic enforcement overhead: acceptable
