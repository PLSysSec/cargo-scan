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

<!-- Short answer -->

2. Permissions

<!-- Short answer -->

3. Transitive risk

<!-- Short answer -->

4. Feasibility of automated analysis

- Spec: <!-- Short answer -->
- Static analysis: <!-- Short answer -->
- Dynamic enforcement overhead: <!-- Short answer -->
