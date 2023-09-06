#!/usr/bin/env python3
"""
Simple script to make sure the data in the test crates CSV file
is up to date with the actual number of test crate directories.
"""

import subprocess

TEST_CRATES_CSV = "data/crate-lists/test-crates.csv"
TEST_CRATES_DIR = "data/test-packages"

def run_shell(cmd):
    return subprocess.run(cmd, shell=True, check=True, capture_output=True).stdout

## Automatically update the data in TEST_CRATES_CSV

run_shell(f"echo name > {TEST_CRATES_CSV}")
run_shell(f"ls -1 {TEST_CRATES_DIR} >> {TEST_CRATES_CSV}")

## Checksum -- now that we are updating automatically, this is just a sanity check

n_tests = int(run_shell(f"wc -l < {TEST_CRATES_CSV}")) - 1
n_files = int(run_shell(f"ls -1 {TEST_CRATES_DIR} | wc -l"))

assert n_tests == n_files, "error: checksum failed!"
