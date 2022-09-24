"""
Script to download Cargo crate source code and analyze module-level imports.
"""

import logging
import os
import re
import subprocess
import time

# ===== Constants =====

PACKAGES_DIR = "packages"
SRC_DIR = "src"
RESULTS_DIR = "results"

OF_INTEREST = [
    "std::fs",
    "std::net",
    "std::os",
    "std::path",
]

CRATES = ["rand", "syn"]

# ===== Logging setup =====

logging.basicConfig(level=logging.INFO)

# ===== Main script =====

def download_crate(crate):
    target = os.path.join(PACKAGES_DIR, crate)
    if os.path.exists(target):
        logging.info(f"Found existing crate: {target}")
    else:
        logging.info(f"Downloading crate: {target}")
        subprocess.run(["cargo", "download", "-x", crate, "-o", target])

def save_results(crate, results):
    timestr = time.strftime("%Y%m%d_%H%M%S")
    results_file = f"{timestr}_{crate}.csv"
    results_path = os.path.join(RESULTS_DIR, results_file)
    logging.info(f"Saving results to {results_path}")
    with open(results_path, 'a') as fh:
        for line in results:
            fh.write(line + '\n')

def of_interest(line):
    found = None
    for p in OF_INTEREST:
        if re.match(p, line):
            if found is not None:
                logging.warning(
                    f"Line matched multiple patterns of interest: {line}"
                )
            found = p
    return found

def parse_use(crate, root, file, line, results):
    interest = of_interest(line)
    if interest is None:
        logging.debug(f"Skipping: {line}")
    elif m := re.fullmatch("use ([^{}]*){([^{}]*)};\n", line):
        prefix = m[1]
        for suffix in m[2].replace(' ', '').split(','):
            results.append(f"{crate}, {interest}, {root}, {file}, {prefix}{suffix}")
    elif m := re.fullmatch("use ([^{}]*)\n", line):
        results.append(f"{crate}, {interest}, {root}, {file}, {m[1]}")
    else:
        logging.warning(f"Unable to parse 'use' line: {line}")

def scan_file(crate, root, file, results):
    filepath = os.path.join(root, file)
    logging.debug(f"Scanning file: {filepath}")
    with open(filepath) as fh:
        for line in fh:
            if re.fullmatch("use .*\n", line):
                parse_use(crate, root, file, line, results)

def scan_crate(crate):
    results = []
    logging.info(f"Scanning crate: {crate}")
    src = os.path.join(PACKAGES_DIR, crate, SRC_DIR)
    for root, dirs, files in os.walk(src):
        for file in files:
            if os.path.splitext(file)[1] == ".rs":
                scan_file(crate, root, file, results)
    return results

# ===== Entrypoint =====

for crate in CRATES:
    download_crate(crate)
    results = scan_crate(crate)
    save_results(crate, results)
