# env_logger

Audited by: Caleb Stanford
Date: 2022-09-28

Top 100 most-downloaded crates

## List of imports (2)

```
src, lib.rs, std::env
src/filter, mod.rs, std::env
```

## Analysis

Also uses `std::env::var`, but in a less obviously safe manner: it contains a method which calls `env::var` on a program variable. It’s less clear just looking at the code whether it accesses some limited finite set of environment variables or whether it can read arbitrary env variables.

Note: `mod.rs` also uses `env` to set (mostly fixed) var names.

## Security summary

1. Security risks

Dangerous if used with the wrong environment variables

2. Permissions

Environment variable access

3. Transitive risk

Likely no in most cases

4. Feasibility of automated analysis

- Spec: maybe project-dependent
- Static analysis: unsure
- Dynamic enforcement overhead: unsure
