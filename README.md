# Cargo Scan

`cargo scan` is a supply chain auditing tool for Cargo (Rust) dependencies using static analysis.
It can also be used in tandem with [cargo vet](https://mozilla.github.io/cargo-vet/).

**⚠️ `cargo scan` is under active development. All interfaces are currently unstable.**

## Installation

Make sure you have [Rust](https://www.rust-lang.org/tools/install), then run `make install`.

This installs [cargo-download](https://crates.io/crates/cargo-download) and builds the Rust source.
Installation has been tested on Mac OS (Monterey) and Linux (Arch Linux).

## Quick-start

### Obtaining a crate

To use Cargo Scan you first need a crate.
You can either:
- Fetch an existing crate from [crates.io](crates.io):
  ```
  cargo download -x <crate name>
  ```
- Use one of the provided test crates in `data/test-crates`
- Provide your own (given the directory to the source files)

## Running a scan

To scan a crate, looking for dangerous function calls:
```
cargo run <path to crate>
```

Crates can be put anywhere, but are generally placed in `data/packages` for our scripting. For example,
```
cargo run data/packages/num_cpus
cargo run data/test-packages/permissions-ex
```

## Running an audit

To audit a package:
```
cargo run --bin audit <path to crate> crate.policy
```

## Unit tests

- Run `cargo test` to run Rust unit tests

- Run `make test` to re-run the tool on all our test packages, whose results are in `data/results` and placed under version control to check for any regressions.

## Running an experiment

You can also run `./scripts/scan.py -h` to see options for running an experiment; this is useful for running a scan on a large list of crates, e.g. the top 100 crates on crates.io or your own provided list. Alternatively, see `Makefile` for some pre-defined experiments to run, such as `make top10`.
