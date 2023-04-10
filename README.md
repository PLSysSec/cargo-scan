# Cargo Scan

`cargo scan` is a supply chain auditing tool for Cargo (Rust) dependencies using static analysis.
It can also be used in tandem with [cargo vet](https://mozilla.github.io/cargo-vet/).

`cargo scan` is under active development and all interfaces should be assumed to be unstable.

## Installation

Make sure you have [Rust](https://www.rust-lang.org/tools/install), then run `make install`.

This installs [cargo-download](https://crates.io/crates/cargo-download) and builds the Rust source.
Installation has been tested on Mac OS (Monterey) and Linux (Arch Linux).

## Quick-start

### Obtaining a crate

To use Cargo Scan you first need a crate. You can either:
- use our script to download one automatically:
```
./scripts/scan.py -c <crate name>
```
- use one of the test crates in `data/test-crates`, or
- download your own Rust crate and put it in a folder somewhere.

The `scan.py` script simply calls `cargo download` on the crate and puts it in `data/packages`, so you can also run `cargo download` yourself.

## Running a scan

To scan a crate, looking for dangerous function calls:
```
cargo run --bin scan <path to crate>
```

Crates can be put anywhere, but are generally placed in `data/packages` for our scripting. For example,
```
cargo run --bin scan data/packages/num_cpus
cargo run --bin scan data/test-packages/permissions-ex
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
