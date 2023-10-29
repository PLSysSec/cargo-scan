#!/usr/bin/env python3

"""
Script to check everything is installed correctly
"""

import subprocess
import sys

# ===== Check requirements =====

MIN_PYTHON = (3, 0)
if sys.version_info < MIN_PYTHON:
    version = f"{MIN_PYTHON[0]}.{MIN_PYTHON[1]}"
    found = f"{sys.version_info.major}.{sys.version_info.minor}"
    sys.exit(f"Error: Python {version} or later is required (found {found}).")

def check_installed(cmd, test_arg="--version", check_exit_code=True):
    args = cmd + [test_arg]
    try:
        subprocess.run(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=check_exit_code)
    except Exception as e:
        sys.exit(f"missing dependency: {cmd} (run `make install`)")

# Dependencies
RUSTC = ["rustc"]
CARGO = ["cargo"]
CARGO_DOWNLOAD = CARGO + ["download"]

# Uncomment to enable debug checks
# CARGO_SCAN = ["./target/debug/scan"]
# Uncomment for release mode
CARGO_SCAN = ["./target/release/scan"]

check_installed(RUSTC)
check_installed(CARGO)
check_installed(CARGO_SCAN)
check_installed(CARGO_DOWNLOAD, check_exit_code=False)

# Unchecked dependencies
# CP = ["cp"]
# OPEN = ["open"]

print("Everything is installed correctly")
