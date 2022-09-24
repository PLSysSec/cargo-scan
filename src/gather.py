"""
Script to download Cargo crate source code and analyze module-level imports.
"""

import logging
import os
import re
import subprocess
import time

PACKAGES_DIR = "packages"
SRC_DIR = "src"
RESULTS_DIR = "results"

logging.basicConfig(level=logging.INFO)

# Main script

def download_crate(crate):
    target = os.path.join(PACKAGES_DIR, crate)
    if os.path.exists(target):
        logging.info(f"found existing crate: {target}")
    else:
        logging.info(f"downloading crate: {target}")
        subprocess.run(["cargo", "download", "-x", crate, "-o", target])

def save_results(crate, results):
    timestr = time.strftime("%Y%m%d_%H%M%S")
    results_file = f"{timestr}_{crate}.csv"
    results_path = os.path.join(RESULTS_DIR, results_file)
    logging.info(f"Saving results to {results_path}")
    with open(results_path, 'a') as fh:
        for line in results:
            fh.write(line + '\n')

def parse_use(crate, root, file, line, results):
    if m := re.fullmatch("use ([^{}]*){([^{}]*)};\n", line):
        prefix = m[1]
        for suffix in m[2].replace(' ', '').split(','):
            results.append(f"{crate}, {root}, {file}, {prefix}{suffix}")
    elif m := re.fullmatch("use ([^{}]*)\n", line):
        results.append(f"{crate}, {root}, {file}, {m[1]}")
    else:
        logging.warning(f"Unable to parse 'use' line: {line}")

def scan_file(crate, root, file, results):
    filepath = os.path.join(root, file)
    logging.debug(f"scanning file: {filepath}")
    with open(filepath) as fh:
        for line in fh:
            if re.fullmatch("use .*\n", line):
                parse_use(crate, root, file, line, results)

def scan_crate(crate):
    results = []
    logging.info(f"scanning crate: {crate}")
    src = os.path.join(PACKAGES_DIR, crate, SRC_DIR)
    for root, dirs, files in os.walk(src):
        for file in files:
            if os.path.splitext(file)[1] == ".rs":
                scan_file(crate, root, file, results)
    return results

CRATES = ["rand"]
# "syn"
for crate in CRATES:
    results = scan_crate(crate)
    save_results(crate, results)
