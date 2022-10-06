Audited by: Caleb Stanford
Date: 2022-10-05

Top 100 most downloaded crates.

## List of imports (2)

```
lib.rs, std::env
lib.rs, std::process::Command
```

## Analysis

Similar to `rustc_version`, `autocfg`, and `rustversion`,
this is just a crate for determining the current Rust version.
For this crate, the check is done at runtime, not via
a proc macro or attribute, and it is non-panicking.

Command is used for only a hardcoded specific call:
`Command::new(rustc).arg("--verbose").arg("--version").output().ok()`

And `std::env` is used only for the CARGO_ENCODED_RUSTFLAGS variable.

## Security summary

1. Security risks

None

2. Permissions

Requires permission to run a single call to `rustc --verbose --version` and
collect the output; and read-only access to the `CARGO_ENCODED_RUSTFLAGS` var.

3. Transitive risk

None

4. Feasibility of automated analysis

- Spec: project-independent
- Static analysis: feasible
- Dynamic enforcement overhead: likely acceptable
