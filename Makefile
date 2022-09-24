.PHONY: build
.DEFAULT: build

build:
	python3 src/scan.py

clean:
	rm -rf packages/
	mkdir packages/
	touch packages/.gitkeep
