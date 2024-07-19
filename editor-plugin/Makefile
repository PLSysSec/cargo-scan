.PHONY: all build_server build_client package clean

BINARY_NAME=lang_server
CARGO_SCAN_DIR=../
OUT_DIR=./out
CLIENT_DIR=./  #$$(pwd)

all: build_server build_client package

build_server:
	@echo "Building Cargo Scan LSP server"
	@cd $(CARGO_SCAN_DIR) && cargo build --release --workspace
	@mkdir -p $(OUT_DIR)
	@cp $(CARGO_SCAN_DIR)/target/release/$(BINARY_NAME) $(OUT_DIR)

build_client:
	@echo "Building Cargo Scan LSP client"
	@cd $(CLIENT_DIR) && npm install
	@cd $(CLIENT_DIR) && npm run build-release

package: build_server build_client
	@echo "Packaging Cargo Scan's VSCode extension"
	@npm run package

clean:
	#@cd $(CARGO_SCAN_DIR) && cargo clean
	@rm -rf $(OUT_DIR)
	@cd $(CLIENT_DIR) && npm run clean
	@rm -f *.vsix


