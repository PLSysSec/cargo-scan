.PHONY: build
.DEFAULT: cargo-scan

cargo-scan:
	python3 experiments/scan.py

clean:
	rm -rf experiments/packages/
	mkdir experiments/packages/
	touch experiments/packages/.gitkeep
