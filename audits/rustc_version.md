# rustc_version

Audited by: Caleb Stanford
Date: 2022-09-28, updated 2022-10-05

Top 100 most-downloaded crates

## List of imports (2)

```
lib.rs, std::process::Command
lib.rs, std::env
```

## Analysis

Uses `std::process::Command`, which is in general dangerous, but in this
case it’s only using the command to figure out the rust version (see
`pub fn version_meta()`), and can’t have any other side effects.

However, interestingly, there's a wrapper around this in the `VersionMeta`
struct that is marked `pub` (probably shouldn't be!) and can be called on any
arbitrary command: `VersionMeta::for_command(cmd)`.

## Security summary

1. Security risks

`VersionMeta::for_command` executes an arbitrary command even though this
functionality is not needed

2. Permissions

Read-only access to the `RUSTC` environment variable;
shell access to run the fixed command `rustc -vV`
(or whatever `RUSTC` is)

3. Transitive risk

Transitive risk if `VersionMeta::for_command` is used inappropriately

4. Feasibility of automated analysis

- Spec: project-independent
- Static analysis: feasible
- Dynamic enforcement overhead: likely acceptable
