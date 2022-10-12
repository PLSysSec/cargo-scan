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

Minimal

2. Permissions

Environment variable read/write access

3. Transitive risk

No

4. Feasibility of automated analysis

- Spec: project-independent
- Static analysis: feasible
- Dynamic enforcement overhead: OK
