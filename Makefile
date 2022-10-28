.PHONY: cargo-scan test full full-extra clean
.DEFAULT: cargo-scan

SCAN = python3 experiments/scan.py

cargo-scan:
	$(SCAN) 100

test:
	$(SCAN) all -t -vvv

full: test
	$(SCAN) 10
	$(SCAN) 100

full-extra: full
	$(SCAN) 1000
	$(SCAN) 10000

clean:
	# Warning: this deletes all downloaded packages and experiment results
	# not under version control!
	# Run make full to redownload and regenerate results.
	rm -rf data/packages/
	mkdir data/packages/
	touch data/packages/.gitkeep
	rm -rf data/results/
	mkdir data/results/
