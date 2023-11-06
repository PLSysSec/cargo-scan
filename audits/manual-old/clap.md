# [clap](https://docs.rs/clap/latest/clap/index.html)

Audited by: Caleb Stanford
Date: 2022-10-05

Top 100 most downloaded crates.

## List of imports (3)

```
src/builder, arg.rs, std::env
src/builder, command.rs, std::env
src/builder, command.rs, std::path::Path
```

## Analysis

`clap` is widely used for CLI argument parsing (and what `structopt` is based on).
I believe that it also now implements most of the functionality of `structopt`,
making `clap` obsolete.

The main `Command` API needs access to `env::args_os` to get the arguments
passed to the program when it was run. It also does some basic
non-effectful Path manipulation.
Additionally, the `Arg` API (arguments added to `Command`) needs access
to `env` because it is possible to specify that an argument
should be obtained from an environment var if it is not present.

Specifically in a typical use: if you implement `Parser` (the analog of
`StructOpt`) for `MyStruct`, then `MyStruct::parse()` eventually calls
`env::args_os` to get the arguments (through several layers of abstraction
including the `CommandFactory` trait that I don't understand the purpose
of).

## Security summary

1. Security risks

None, unless dealing with sensitive arguments to the CLI
or sensitive environment variables.

2. Permissions spec

- Read access to the `env::args_os()` contents of the CLI call
- Read access to any environment variables desired by the user

3. Transitive risk

None

4. Automation feasibility

- Spec: feasible, sometimes project-specific
- Static analysis: feasible
- Dynamic enforcement overhead: acceptable
