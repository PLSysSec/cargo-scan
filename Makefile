.PHONY: install top10 top100 top1000 top10000 test mozilla small medium large clean
.DEFAULT_GOAL := top10

install:
	cargo install cargo-download
	git submodule init
	git submodule update
	cd mirai/MIRAI && cargo install --locked --path ./checker
	cd rust-src && cargo build && cargo build --release

top10:
	./scan.py -i data/crates-top10.csv -o top10

top100:
	./scan.py -i data/crates-top100.csv -o top100

top1000:
	./scan.py -i data/crates-top1000.csv -o top1000

top10000:
	./scan.py -i data/crates-top10000.csv -o top10000

test:
	./scan.py -t -i data/test-crates.csv -o test -vvv

mozilla:
	./scan.py -i data/mozilla-exempt.csv -o mozilla-exempt
	./scan.py -i data/mozilla-audits.csv -o mozilla-audits

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
