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

## Quick-start

To use Cargo Scan you first need a Rust crate somewhere on your system.
To scan a crate, you run the binary (from this repository), and provide it a path to the crate.

### Running an audit

The following runs an audit of the crate:
```
cargo run <path to crate> mycrate.audit -i
```

The tool will show you dangerous effects found in the crate, one at a time.
To go to the next effect, type `l`.

For example, you can download a crate and run
```
cargo download -x fs-extra
cargo run fs_extra-1.3.0/ fs-extra.audit -i
```

Or you can run on a provided test crate in `data/test-packages`:
```
cargo run data/test-packages/permissions-ex permissions-ex.audit -i
```

If the command is run a second time, it continues the existing audit.
To instead overwrite the existing audit, use `-f`.
To review the audit, use `-r`.

### Scan with CSV output

If you don't want to perform an audit, you can also simply get the list of
effects found in a basic CSV format for further inspection and analysis.
To run the tool this way, use the `scan` binary:
```
cargo run --bin scan <path to crate>
```

This should print a list of effects, one per line.
The last four items on each line give the directory, file, line, and column where the effect occurs.
The beginning of the line gives the crate name, the function body and callee that contains the effect, and the effect type or pattern that it matches.

## Detailed instructions

Please see the file `AUDITING.md` for further instructions about auditing.

## Other usage

### Running the unit tests

- Run `cargo test` to run Rust unit tests

- Run `make test` to re-run the tool on all our test packages, whose results are in `data/results` and placed under version control to check for any regressions.

### Running an experiment

You can also run `./scripts/scan.py -h` to see options for running an experiment; this is useful for running a scan on a large list of crates, e.g. the top 100 crates on crates.io or your own provided list. Alternatively, see `Makefile` for some pre-defined experiments to run, such as `make top10`.
