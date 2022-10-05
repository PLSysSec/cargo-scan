# autocfg

One of two crates in the top 10 most downloaded crates with dangerous imports.

## 10 imports

```
lib.rs, std::env
tests.rs, std::env
lib.rs, std::fs
lib.rs, std::path::Path
lib.rs, std::path::PathBuf
tests.rs, std::path::Path
version.rs, std::path::Path
lib.rs, std::process::Command
version.rs, std::process::Command
lib.rs, std::process::Stdio
```

## Analysis

- std::env: **fixed set of environment variables:**
OUT_DIR, RUSTC, TARGET, HOST, RUSTFLAGS
CARGO_ENCODED_RUSTFLAGS, CARGO_TARGET_DIR
Only in cfg(test): TESTS_TARGET_DIR

- std::fs: for fs::metadata; Path functions .is_dir(), .contains() etc.

  As far as I can tell in the crate itself this is only called on the environment var OUT_DIR, but the function `with_dir` is public so the user could, if they want to for some reason, call it on something else (this seems unlikely in practice).

  In the dir_contains_target(), does the .contains() call there just resolve to str .contains()? That may make that function side-effect-free.

- Command: **probably fixed (though large) set of possible command args.**
  The interesting one. Used to call `rustc` (just a probing call) with a particular set
  of args, some provided by user input (and not necessarily validated!).
  The actual list of arguments can be complex and various different probes
  are possible.

  Similarly version.rs uses Command to probe `rustc --version`.

- Stdio: used to set rustc's stdin to Stdio(). That seems benign, in fact I'm not sure Stdio is ever problematic, it's solely used as an arg to Command.

## Security summary

- I believe the crate very easily allows executing arbitrary code at build time with an appropriate call to one of the `emit` methods.

- The crate needs shell access to call commands of a fixed form, namely, calls to `rustc` with various arguments.

- The crate needs read-only access to a fixed set of environment variables and a fixed set of filepaths.

- It may also need access to additional filepaths if the user passes them in via function calls.

- In typical use cases, it needs this access at build time only. (`build.rs`)
