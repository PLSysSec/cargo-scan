# [pkg-config](https://docs.rs/pkg-config/0.3.25/pkg_config/)

Audited by: Caleb Stanford

Date: 2022-10-12

72nd most downloaded crate

## List of imports (4)

```
src, lib.rs, std::env
src, lib.rs, std::path::PathBuf
src, lib.rs, std::process::Command
src, lib.rs, std::process::Output
```

## Analysis

Used for finding and checking the existence of system binaries at build time.
Calls out to the shell.
I'm not sure if it actually can run any commands or if there
are code injection attacks possible.
I don't think it can be used to actually link the binaries into the application
(e.g. through C FFI bindings).

## Security summary

1. Security risks

Possible shell dangers

2. Permissions

Build time only: shell, filesystem, and env var access

3. Transitive risk

Unsure

4. Automation feasibility

- Spec: somewhat project-dependent
- Static analysis: potentially feasible
- Dynamic enforcement overhead: acceptable
