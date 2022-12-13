.PHONY: install top10 top100 top1000 top10000 test mozilla small medium large clean
.DEFAULT_GOAL := install

install:
	cargo install cargo-download
	git submodule init
	git submodule update
	cd mirai/MIRAI && cargo install --locked --path ./checker
	cd rust-src && cargo build && cargo build --release

top10:
	./scan.py -i data/crate-lists/top10.csv -o top10

top100:
	./scan.py -i data/crate-lists/top100.csv -o top100

top1000:
	./scan.py -i data/crate-lists/top1000.csv -o top1000

top10000:
	# Note: this actually contains only 9999 crates at the moment.
	./scan.py -i data/crate-lists/top10000.csv -o top10000

test:
	./scan.py -t -i data/crate-lists/test-crates.csv -o test -vvv

mozilla:
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
