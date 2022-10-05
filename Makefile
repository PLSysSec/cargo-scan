.PHONY: build
.DEFAULT: cargo-scan

cargo-scan:
	python3 experiments/scan.py

full:
	python3 experiments/scan.py 10
	python3 experiments/scan.py 100

test:
	python3 experiments/scan.py -t -vvv

clean:
	rm -rf experiments/packages/
	mkdir experiments/packages/
	touch experiments/packages/.gitkeep
