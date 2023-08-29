# Cargo Scan

`cargo scan` is a supply chain auditing tool for Cargo (Rust) dependencies using static analysis.
It can also be used in tandem with [cargo vet](https://mozilla.github.io/cargo-vet/).

**⚠️ `cargo scan` is under active development. All interfaces are currently unstable.**

## Installation

1. Make sure you have [Rust](https://www.rust-lang.org/tools/install)
2. Run `rustup update` -- the build has been known to crash on older versions of Rust.
3. Run `make install`.

This installs [cargo-download](https://crates.io/crates/cargo-download) and builds the Rust source.
Installation has been tested on Mac OS (Monterey) and Linux (Arch Linux).

## Quick-start: running a scan

To use Cargo Scan you first need a Rust crate somewhere on your system. To scan a crate, looking for dangerous function calls:
```
cargo run <path to crate>
```

For example, you can download a crate and run
```
cargo download -x fs-extra
cargo run fs_extra-1.3.0/
```

Or you can run on a provided test crate in `data/test-packages`:
```
cargo run data/test-packages/permissions-ex
```

## Running an audit

To audit a package:
```
cargo run --bin audit <path to crate> crate.policy
```

Auditing is WIP, for more information please see the file `AUDITING.md`.

## Other usage

### Running the unit tests

- Run `cargo test` to run Rust unit tests

- Run `make test` to re-run the tool on all our test packages, whose results are in `data/results` and placed under version control to check for any regressions.

### Running an experiment

You can also run `./scripts/scan.py -h` to see options for running an experiment; this is useful for running a scan on a large list of crates, e.g. the top 100 crates on crates.io or your own provided list. Alternatively, see `Makefile` for some pre-defined experiments to run, such as `make top10`.
