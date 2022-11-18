# Cargo Scan

`cargo scan` is an under-development experimental tool to scan and analyze Cargo (Rust) dependencies for security and auditing purposes.
It can also be used in tandem with [cargo vet](https://mozilla.github.io/cargo-vet/).

The tool is currently implemented as a top-level Python wrapper (`scan.py`), but this may change later.
This repository also collects related experiments, test crates, and experimental data.

## Installation

Make sure you have Python 3 and [Rust](https://www.rust-lang.org/tools/install), then run `make install`.

This installs [cargo-download](https://crates.io/crates/cargo-download) and our fork of [MIRAI](https://github.com/facebookexperimental/MIRAI).
Installation has currently been tested on Mac OS (Monterey) and Linux (Arch Linux).

If you want to use the `-g` option, you also need to install [graphviz](https://graphviz.org/download/): on Mac, `brew install graphviz`.

## Running an experiment

To scan a crate, looking for dangerous import patterns:
```
python3 scan.py -c <crate name>
```

To scan a crate, using MIRAI to locate dangerous functions in the call graph (this can take a bit of time):
```
python3 scan.py -c <crate name> -m
```

Both of the above will download the requested crate to `data/packages`. To instead scan a test crate in `data/test-packages`:
```
python3 scan.py -c <crate name> -t
```

You can play around with this by adding your own example Rust crates in `data/test-packages`.

For additional options, run `python3 scan.py -h` or run one of the pre-defined experiments in `Makefile`.
