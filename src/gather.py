"""
Script to download Cargo crate source code and analyze module-level imports.
"""

import os
import re
import subprocess

PACKAGES_DIR = "packages"
SRC_DIR = "src"

def download_crate(name):
    target = os.path.join(PACKAGES_DIR, name)
    if os.path.exists(target):
        print(f"found existing crate: {target}")
    else:
        print(f"downloading crate: {target}")
        subprocess.run(["cargo", "download", "-x", name, "-o", target])

def scan_file(root, file):
    filepath = os.path.join(root, file)
    print(f"scanning file: {filepath}")
    with open(filepath) as fh:
        for line in fh:
            if re.match("use .*", line):
                print(root, line)

def scan_crate(name):
    print(f"scanning crate: {name}")
    src = os.path.join(PACKAGES_DIR, name, SRC_DIR)
    for root, dirs, files in os.walk(src):
        for file in files:
            if os.path.splitext(file)[1] == ".rs":
                scan_file(root, file)

download_crate("rand")
download_crate("syn")
scan_crate("rand")
# scan_crate("syn")
