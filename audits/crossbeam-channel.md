# crossbeam-channel

Audited by: Caleb Stanford
Date: 2022-09-28

Top 100 most-downloaded crates

## List of imports (1)

```
crossbeam-channel, std::process, src, counter.rs, std::process
```

## Analysis

Imports `std::process` purely for two calls to `std::process::abort()`.  I’m not sure why the implementation needs to call abort rather than panic? On the other hand, this seems probably generally safe, but do I want to import code that might bypass my objects’ destructors?

## Security summary

1. Security risks

`process::abort()` can bypass destructor runs, saving work-in-progress,
or other important exiting behavior.
This can be destructive but probably can't be used for anything malicious.

2. Permissions

Abort the process

3. Transitive risk

No

4. Feasibility of automated analysis

- Spec: project-independent
- Static analysis: likely feasible
- Dynamic enforcement overhead: maybe acceptable
