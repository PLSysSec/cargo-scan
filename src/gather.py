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

CRATES = [
    "rand",
    "syn",
    "rand_core",
    "libc",
    "cfg-if",
    "quote",
    "proc-macro2",
    "unicode-xid",
    "serde",
    "autocfg",
    "bitflags",
    "rand_chacha",
    "log",
    "lazy_static",
    "itoa",
    "getrandom",
    "serde_derive",
    "memchr",
    "time",
    "base64",
    "serde_json",
    "num-traits",
    "regex",
    "smallvec",
    "regex-syntax",
    "cc",
    "parking_lot_core",
    "version_check",
    "parking_lot",
    "strsim",
    "ryu",
    "aho-corasick",
    "semver",
    "bytes",
    "crossbeam-utils",
    "byteorder",
    "generic-array",
    "lock_api",
    "scopeguard",
    "digest",
    "clap",
    "once_cell",
    "atty",
    "block-buffer",
    "num_cpus",
    "hashbrown",
    "num-integer",
    "textwrap",
    "percent-encoding",
    "url",
]

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

def save_results(results):
    timestr = time.strftime("%Y%m%d_%H%M%S")
    results_file = f"{timestr}_all.csv"
    results_path = os.path.join(RESULTS_DIR, results_file)
    logging.info(f"Saving results to {results_path}")
    with open(results_path, 'a') as fh:
        for line in results:
            fh.write(line + '\n')

def of_interest(line):
    found = None
    for p in OF_INTEREST:
        if re.search(p, line):
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

def scan_crate(crate, results):
    logging.info(f"Scanning crate: {crate}")
    src = os.path.join(PACKAGES_DIR, crate, SRC_DIR)
    for root, dirs, files in os.walk(src):
        for file in files:
            if os.path.splitext(file)[1] == ".rs":
                scan_file(crate, root, file, results)

# ===== Entrypoint =====

results = []
for crate in CRATES:
    download_crate(crate)
    scan_crate(crate, results)
save_results(results)
