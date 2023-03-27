#!/usr/bin/env python3

"""
View call graph for a crate using MIRAI
"""

import argparse
import logging
import os
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
CARGO_MIRAI = CARGO + ["mirai"]
CARGO_DOWNLOAD = CARGO + ["download"]
GRAPHVIZ_DOT = ["dot"]

check_installed(RUSTC)
check_installed(CARGO)
check_installed(CARGO_MIRAI)
check_installed(CARGO_DOWNLOAD, check_exit_code=False)
check_installed(GRAPHVIZ_DOT, check_exit_code=False)

# Unchecked dependencies
CP = ["cp"]
OPEN = ["open"]

# ===== Additional constants & config =====

CRATES_DIR = "data/packages"
TEST_CRATES_DIR = "data/test-packages"

MIRAI_CONFIG = "mirai/config.json"
MIRAI_FLAGS_KEY = "MIRAI_FLAGS"
MIRAI_FLAGS_VAL = f"--call_graph_config ../../../{MIRAI_CONFIG}"

# Color logging output
logging.addLevelName(logging.INFO, "\033[0;32m%s\033[0;0m" % "INFO")
logging.addLevelName(logging.WARNING, "\033[0;33m%s\033[0;0m" % "WARNING")
logging.addLevelName(logging.ERROR, "\033[0;31m%s\033[0;0m" % "ERROR")

# ===== Main script =====

def download_crate(crates_dir, crate, test_run):
    target = os.path.join(crates_dir, crate)
    if os.path.exists(target):
        logging.debug(f"Found existing crate: {target}")
    else:
        if test_run:
            logging.warning(f"Crate not found during test run: {target}")
        else:
            logging.info(f"Downloading crate: {target}")
            subprocess.run(CARGO_DOWNLOAD + ["-x", crate, "-o", target], check=True)

def run_mirai(crate, crate_dir):
    os.environ[MIRAI_FLAGS_KEY] = MIRAI_FLAGS_VAL
    subprocess.run(CARGO + ["clean"], cwd=crate_dir, check=True)
    command = CARGO_MIRAI
    logging.debug(f"Calling MIRAI: {command} in {crate_dir}")
    proc = subprocess.Popen(command, cwd=crate_dir)

def view_callgraph_mirai(crate_dir):
    subprocess.run(GRAPHVIZ_DOT + ["-Tpng", "graph.dot", "-o", "graph.png"], cwd=crate_dir, check=True)
    subprocess.run(OPEN + ["graph.png"], cwd=crate_dir, check=True)

# ===== Entrypoint =====

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('-c', '--crate', help="Crate name to scan", required=True)
    parser.add_argument('-t', '--test-run', action="store_true", help=f"Test run: use existing crate in {TEST_CRATES_DIR} instead of downloading via cargo-download")
    parser.add_argument('-v', '--verbose', action="count", help="Verbosity level: v=err, vv=warning, vvv=info, vvvv=debug, vvvvv=trace (default: info)", default=0)

    args = parser.parse_args()

    if args.verbose > 4:
        logging.error("verbosity only goes up to 4 (-vvvv)")
        sys.exit(1)
    log_level = [logging.INFO, logging.ERROR, logging.WARNING, logging.INFO, logging.DEBUG][args.verbose]
    logging.basicConfig(level=log_level)
    logging.debug(args)

    crate = args.crate
    if args.test_run:
        crates_dir = TEST_CRATES_DIR
    else:
        crates_dir = CRATES_DIR

    logging.info(f"=== Generating call graph for {crate} in {crates_dir} ===")

    try:
        download_crate(crates_dir, crate, args.test_run)
    except subprocess.CalledProcessError as e:
        logging.error(f"cargo-download failed for crate: {crate} ({e})")
        sys.exit(1)

    crate_dir = os.path.join(crates_dir, crate)
    run_mirai(crate, crate_dir)

    logging.info("=== Generating call graph as a PNG ===")
    view_callgraph_mirai(crate_dir)

if __name__ == "__main__":
    main()
