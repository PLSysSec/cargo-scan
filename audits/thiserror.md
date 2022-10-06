# [thiserror](https://docs.rs/thiserror/latest/thiserror/)

Audited by: Caleb Stanford
Date: 2022-10-05

Top 100 most downloaded crates.

## List of imports (3)

```
display.rs, std::path::Path
display.rs, std::path::PathBuf
display.rs, std::path::self
```

## Analysis

One of Rust's many error handling crates.

It imports `path` features only to implement some custom traits for them
in `display.rs`. This module is very weird though, and not actually used
by the crate (maybe old/depcrated stuff).

## Security summary

1. Security risks

None

2. Permissions

None that I can see

3. Transitive risk

None

4. Automation feasibility

- Spec: trivial and package-independent
- Static analysis: feasible
- Dynamic enforcement overhead: N/A
