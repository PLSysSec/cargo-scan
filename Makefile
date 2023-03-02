.PHONY: install install-mirai checks test top10 top100 top1000 top10000 mozilla small medium large clean
.DEFAULT_GOAL := install

install:
	cargo install cargo-download
	cd rust-src && cargo build && cargo build --release

install-mirai: install
	cargo install cargo-download
	git submodule init
	git submodule update
	cd mirai/MIRAI && cargo install --locked --path ./checker

checks:
	cd rust-src && cargo build
	cd rust-src && cargo test
	cd rust-src && cargo clippy
	cd rust-src && cargo fmt

test: install
	./scan.py -t -i data/crate-lists/test-crates.csv -o test -vvv

top10: install
	./scan.py -i data/crate-lists/top10.csv -o top10

top100: install
	./scan.py -i data/crate-lists/top100.csv -o top100

top1000: install
	./scan.py -i data/crate-lists/top1000.csv -o top1000

top10000: install
	# Note: this actually contains only 9998 crates at the moment.
	./scan.py -i data/crate-lists/top10000.csv -o top10000

mozilla: install
	./scan.py -i data/crate-lists/mozilla-exempt.csv -o mozilla-exempt
	./scan.py -i data/crate-lists/mozilla-audits.csv -o mozilla-audits

small: test top10 top100

medium: small mozilla

large: medium top1000 top10000

clean:
	# Warning: this deletes all downloaded packages and experiment results not under version control!
	# Run make full to redownload and regenerate results.
	@echo "Are you sure you want to continue? [y/N]" && read ans && [ $${ans:-N} = y ]
	rm -rf data/packages/
	mkdir data/packages/
	touch data/packages/.gitkeep
	rm -rf data/results/
	mkdir data/results/
