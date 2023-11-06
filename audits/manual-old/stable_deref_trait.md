# stable_deref_trait

Audited by: Caleb Stanford
Date: 2022-09-28

Top 200 most-downloaded crates

## List of imports (1)

```
lib.rs, std::path::PathBuf
```

## Analysis

imports `std::path::PathBuf` purely to implement a trait for it. This is completely safe: any other code would have to import PathBuf itself before using it.

## Security summary

1. Security risks

None

2. Permissions

None

3. Transitive risk

None

4. Feasibility of automated analysis

- Spec: easy
- Static analysis: feasible
- Dynamic enforcement overhead: none
