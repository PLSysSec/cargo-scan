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

## Running experiments

The file `/experiments/scan.py` contains a script to gather data on module-level Cargo dependencies. It also saves the results to `experiments/results/` under version control. To run the experiment, run `make`.
