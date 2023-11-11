.PHONY: install checks test test-crates-csv test-results top10 top100 top1000 top10000 mozilla small medium large clean
.DEFAULT_GOAL := install

SCAN_ALL := cargo run --release --bin scan_all --
UPDATE_TEST_CRATES_CSV := ./scripts/update_test_crates_csv.py

install:
	- cargo install cargo-download
	cargo build && cargo build --release

checks:
	cargo test
	cargo clippy
	cargo fmt

test-crates-csv:
	$(UPDATE_TEST_CRATES_CSV)

test-results: install test-crates-csv
	$(SCAN_ALL) data/crate-lists/test-crates.csv test -t

test: checks test-results
	- git diff --word-diff data/results/test_all.csv

top10: install
	$(SCAN_ALL) data/crate-lists/top10.csv top10

top100: install
	$(SCAN_ALL) data/crate-lists/top100.csv top100 -n 50

top1000: install
	$(SCAN_ALL) data/crate-lists/top1000.csv top1000 -n 50

top10000: install
	# Note: this actually contains only 9998 crates at the moment.
	$(SCAN_ALL) data/crate-lists/top10000.csv top10000

mozilla: install
	$(SCAN_ALL) data/crate-lists/mozilla-exempt.csv mozilla-exempt
	$(SCAN_ALL) data/crate-lists/mozilla-audits.csv mozilla-audits

small: test-results top10

medium: top100 mozilla

large: top1000 top10000

clean:
	# Warning: this deletes all downloaded packages and experiment results not under version control!
	# Run make full to redownload and regenerate results.
	@echo "Are you sure you want to continue? [y/N]" && read ans && [ $${ans:-N} = y ]
	# Removing...
	# - downloaded packages
	rm -rf data/packages/
	mkdir data/packages/
	touch data/packages/.gitkeep
	# - experimental results
	rm -rf data/results/
	mkdir data/results/
	# - Rust targets
	cargo clean
