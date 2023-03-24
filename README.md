# Cargo Scan

`cargo scan` is a static analysis-assisted supply chain security and auditing tool for Cargo (Rust) dependencies.
It can also be used in tandem with [cargo vet](https://mozilla.github.io/cargo-vet/).

`cargo scan` is under active development.
The easiest way to run the tool right now is the Python wrapper `scan.py`.

## Installation

Make sure you have Python 3 (at least 3.7) and [Rust](https://www.rust-lang.org/tools/install), then run `make install`.

This installs [cargo-download](https://crates.io/crates/cargo-download) and our fork of [MIRAI](https://github.com/facebookexperimental/MIRAI).
It also builds the Rust source.
Installation has currently been tested on Mac OS (Monterey) and Linux (Arch Linux).

If you want to use the `-g` option, you also need to install [graphviz](https://graphviz.org/download/): on Mac, `brew install graphviz`.

## Running a scan

To scan a crate, looking for dangerous function calls:
```
./scan.py -c <crate name>
```

This uses the default backend based on source-code syntax. To scan a crate using the MIRAI backend instead:
```
./scan.py -c <crate name> -m
```

Both of the above will download the requested crate to `data/packages` from [crates.io](crates.io).
To try out the tool on your own example crate, add it as a folder under `data/test-packages`, then run using the `-t` option:
```
./scan.py -c <crate name> -t
```

For additional options, run `./scan.py -h` or run one of the pre-defined experiments that can be found in `Makefile`.
