.PHONY: cargo-scan test full full-extra clean
.DEFAULT: cargo-scan

cargo-scan:
	python3 experiments/scan.py 100

test:
	python3 experiments/scan.py all -t -vvv

full: test
	python3 experiments/scan.py 10
	python3 experiments/scan.py 100

full-extra: full
	python3 experiments/scan.py 1000
	python3 experiments/scan.py 10000

clean:
	# Warning: this deletes all downloaded packages and experiment results
	# not under version control!
	# Run make full to redownload and regenerate results.
	rm -rf data/packages/
	mkdir data/packages/
	touch data/packages/.gitkeep
	rm -rf data/results/
	mkdir data/results/
