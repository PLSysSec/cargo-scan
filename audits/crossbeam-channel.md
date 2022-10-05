# crossbeam-channel

Audited by: Caleb Stanford
Date: 2022-09-28

Top 100 most-downloaded crates

## List of imports (1)

```
crossbeam-channel, std::process, experiments/packages/crossbeam-channel/src, counter.rs, std::process
```

## Analysis

Imports `std::process` purely for two calls to `std::process::abort()`.  I’m not sure why the implementation needs to call abort rather than panic? On the other hand, this seems probably generally safe, but do I want to import code that might bypass my objects’ destructors?

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
