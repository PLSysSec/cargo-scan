# Cargo Vet Experiments

Repository to collect tools and experiments related to the `cargo vet` project.

## Installation

1. Make sure you have Python 3 and [Rust](https://www.rust-lang.org/tools/install).

2. Install [cargo-download](https://crates.io/crates/cargo-download) by running `cargo install cargo-download`

3. Install our fork of [MIRAI](https://github.com/facebookexperimental/MIRAI) by running the following:
```
git submodule init
git submodule update
cd mirai/MIRAI
cargo install --locked --path ./checker
```

## Running an experiment

To scan a crate, looking for dangerous import patterns:
```
python3 scan.py -c <crate name>
```

To scan a crate, using MIRAI to locate dangerous functions in the call graph (this can take a bit of time):
```
python3 scan.py -c <crate name> -m
```

Both of the above will download the requested crate to `data/packages`. To instead scan a test crate in `data/test-packages`, use `-t`, e.g.
```
python3 scan.py -c permissions-ex -t
```

You can play around with this by adding your own example Rust crates in `data/test-packages`.

For additional options, run `python3 scan.py -h` or run one of the pre-defined experiments in `Makefile`.
