# Cargo Vet Experiments

Repository to collect experiments related to `cargo vet` and auditing of Cargo crates.

## To run

The file `/experiments/scan.py` contains a script to gather data on module-level Cargo dependencies. It also saves the results to `experiments/results/` under version control. To run the experiment, run `make`.

## Requirements

- Python 3
- [Rust](https://www.rust-lang.org/tools/install)
- [cargo-download](https://crates.io/crates/cargo-download)
