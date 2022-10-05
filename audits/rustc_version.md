# rustc_version

Audited by: Caleb Stanford
Date: 2022-09-28

Top 100 most-downloaded crates

## List of imports (2)

```
lib.rs, std::process::Command
lib.rs, std::env
```

## Analysis

Uses `std::process::Command`, which is in general very heavyweight/dangerous, but in this case it’s (presumably, I didn’t check) only using the command to do something which figures out the rust version, and can’t have any other side effects. So this is a good example of a crate that should be a roughly safe to abstract as having no important side effects, despite having a dangerous implementation.

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
