"""
Script to download Cargo crate source code and analyze module-level imports.
"""

import os
import re
import subprocess
import time

PACKAGES_DIR = "packages"
SRC_DIR = "src"

RESULTS_DIR = "results"

def download_crate(name):
    target = os.path.join(PACKAGES_DIR, name)
    if os.path.exists(target):
        print(f"found existing crate: {target}")
    else:
        print(f"downloading crate: {target}")
        subprocess.run(["cargo", "download", "-x", name, "-o", target])

def save_results(crate, results):
    timestr = time.strftime("%Y%m%d_%H%M%S")
    results_file = f"{timestr}_{crate}.csv"
    results_path = os.path.join(RESULTS_DIR, results_file)
    print(f"Saving results to {results_path}")
    with open(results_path, 'a') as fh:
        for line in results:
            fh.write(line)

def scan_file(root, file, results):
    filepath = os.path.join(root, file)
    print(f"scanning file: {filepath}")
    with open(filepath) as fh:
        for line in fh:
            if re.match("use .*", line):
                results.append(f"{root}, {line}")

def scan_crate(name):
    results = []
    print(f"scanning crate: {name}")
    src = os.path.join(PACKAGES_DIR, name, SRC_DIR)
    for root, dirs, files in os.walk(src):
        for file in files:
            if os.path.splitext(file)[1] == ".rs":
                scan_file(root, file, results)
    return results

CRATES = ["rand"]
# "syn"
for crate in CRATES:
    results = scan_crate(crate)
    save_results(crate, results)
