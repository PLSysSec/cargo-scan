#!/usr/bin/env python3
"""
Simple script to make sure the data in the test crates CSV file
is up to date with the actual number of test crate directories.
"""

import subprocess

TEST_CRATES_CSV = "data/crate-lists/test-crates.csv"
TEST_CRATES_DIR = "data/test-packages"

n_tests = int(subprocess.run(f"wc -l < {TEST_CRATES_CSV}", capture_output=True, shell=True, check=True).stdout) - 1
n_files = int(subprocess.run(f"ls -1 {TEST_CRATES_DIR} | wc -l", capture_output=True, shell=True, check=True).stdout)

assert n_tests == n_files, f"""
CHECKSUM FAILED: {TEST_CRATES_CSV} is not be up to date! Discrepancy found between:
    {TEST_CRATES_CSV}
    {TEST_CRATES_DIR}
Please add lines to {TEST_CRATES_CSV} to match the folders in test-packages."""
