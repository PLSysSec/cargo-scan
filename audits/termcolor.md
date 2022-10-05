# termcolor

Audited by: Caleb Stanford
Date: 2022-09-28

Top 100 most-downloaded crates

## List of imports (1)

```
lib.rs, std::env
```

## Analysis

Imports `std::env` to get environment variables: `env::var_os` and `env::var`. It specifically only cares about two environment variables, `TERM` and `TERM_OS`. If importing the crate in this case we can summarize its expected behavior with a finite list of environment variables it wants to access.

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
