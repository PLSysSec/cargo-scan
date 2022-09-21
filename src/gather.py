"""
Script to download Cargo crate source code and analyze module-level imports.
"""

import subprocess

PACKAGES_DIR = "packages"

def download_crate(name):
    subprocess.run(["cargo", "download", "-x", name, "-o", f"{PACKAGES_DIR}/{name}"])

download_crate("rand")
download_crate("syn")
