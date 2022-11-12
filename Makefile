.PHONY: install top10 top100 top1000 top10000 test mozilla small medium large clean
.DEFAULT: top10

SCAN = python3 scan.py

install:
	cargo install cargo-download
	git submodule init
	git submodule update
	cd mirai/MIRAI && cargo install --locked --path ./checker

top10:
	$(SCAN) -i data/crates-top10.csv -o top10

top100:
	$(SCAN) -i data/crates-top100.csv -o top100

top1000:
	$(SCAN) -i data/crates-top1000.csv -o top1000

top10000:
	$(SCAN) -i data/crates-top10000.csv -o top10000

test:
	$(SCAN) -t -i data/test-crates.csv -o test -vvv

mozilla:
	$(SCAN) -i data/mozilla-exempt.csv -o mozilla-exempt
	$(SCAN) -i data/mozilla-audits.csv -o mozilla-audits

small: test top10 top100

medium: small mozilla

large: medium top1000 top10000

clean:
	# Warning: this deletes all downloaded packages and experiment results
	# not under version control!
	# Run make full to redownload and regenerate results.
	rm -rf data/packages/
	mkdir data/packages/
	touch data/packages/.gitkeep
	rm -rf data/results/
	mkdir data/results/
