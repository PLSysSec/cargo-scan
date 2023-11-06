# proc-macro2

Audited by: Caleb Stanford
Date: 2022-10-05

One of two crates in the top 10 most downloaded crates with dangerous imports.

## List of imports (4)

```
fallback.rs, std::path::Path
fallback.rs, std::path::PathBuf
lib.rs, std::path::PathBuf
wrapper.rs, std::path::PathBuf
```

## Analysis

Uses Path/PathBuf purely to represent paths which, as far as I can tell, don't
actually access the filesystem. Most of the .path() methods and features are
marked as unstable. The paths are inside a SourceFile abstraction which has a `.is_real()` method to determine if they represent real files or not; it returns `false` for source files defined in this crate.

`proc_macro2` is a wrapper around the compiler's `proc_macro` module.
The method falls back on the compiler's abstractions which do include real
filepaths. It's possible there's some way to trick the Rust macro system
into accessing the file in an unauthorized way, I'm not sure, but if so
that would be possible through basic `proc_macro` and not through the
wrapper.

## Security summary

1. Security risks

Security risks of the crate match any risks associated with procedural
macros in the standard library.

2. Permissions

No permissions needed for the crate itself. Some wrapped `proc_macro`
functionality may need read-only file access.
Permissions only needed at build time.

3. Transitive risk

Likely none.

4. Feasibility of automated analysis

- Spec: project-independent
- Static analysis: feasible
- Dynamic enforcement overhead: acceptable
