.PHONY: install install-mirai checks test top10 top100 top1000 top10000 mozilla small medium large clean
.DEFAULT_GOAL := install

SCAN_PY = ./scripts/scan.py

install:
	cargo install cargo-download
	cargo build && cargo build --release

install-mirai: install
	cargo install cargo-download
	git submodule init
	git submodule update
	cd mirai/MIRAI && cargo install --locked --path ./checker

test: install
	cargo test
	cargo clippy
	cargo fmt
	$(SCAN_PY) -t -i data/crate-lists/test-crates.csv -o test -vvv
	- git diff --word-diff data/results

top10: install
	$(SCAN_PY) -i data/crate-lists/top10.csv -o top10

top100: install
	$(SCAN_PY) -i data/crate-lists/top100.csv -o top100

top1000: install
	$(SCAN_PY) -i data/crate-lists/top1000.csv -o top1000

top10000: install
	# Note: this actually contains only 9998 crates at the moment.
	$(SCAN_PY) -i data/crate-lists/top10000.csv -o top10000

mozilla: install
	$(SCAN_PY) -i data/crate-lists/mozilla-exempt.csv -o mozilla-exempt
	$(SCAN_PY) -i data/crate-lists/mozilla-audits.csv -o mozilla-audits

small: test top10 top100

medium: small mozilla

large: medium top1000 top10000

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
