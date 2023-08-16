#!/usr/bin/env python3

"""
Cargo Scan

Script to download Cargo crate source code and analyze module-level imports.
"""

import argparse
import csv
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
CARGO_DOWNLOAD = CARGO + ["download"]

# Uncomment to enable debug checks
CARGO_SCAN = ["./target/debug/scan"]
# Uncomment for release mode
# CARGO_SCAN = ["./target/release/scan"]
CARGO_SCAN_CSV_HEADER = "crate, fn_decl, callee, effect, dir, file, line, col"

check_installed(RUSTC)
check_installed(CARGO)
check_installed(CARGO_SCAN)
check_installed(CARGO_DOWNLOAD, check_exit_code=False)

# Unchecked dependencies
CP = ["cp"]
OPEN = ["open"]

# ===== Additional constants =====

# Number of progress tracking messages to display
PROGRESS_INCS = 10

CRATES_DIR = "data/packages"
TEST_CRATES_DIR = "data/test-packages"
RUST_SRC = "src"

# Potentially dangerous stdlib imports.
OF_INTEREST_STD = [
    "std::env",
    "std::fs",
    "std::net",
    "std::os",
    "std::path",
    "std::process",
]

# Crates that seem to be a transitive risk.
# This list is manually updated.
OF_INTEREST_OTHER = [
    "libc",
    "winapi",
    "mio::net",
    "mio::unix",
    "tokio::fs",
    "tokio::io",
    "tokio::net",
    "tokio::process",
    "hyper::client",
    "hyper::server",
    "tokio_util::udp",
    "tokio_util::net",
    "socket2",
]

RESULTS_DIR = "data/results"
RESULTS_ALL_SUFFIX = "_all.csv"
RESULTS_PATTERN_SUFFIX = "_pattern.txt"
RESULTS_SUMMARY_SUFFIX = "_summary.txt"

# ===== Utility =====

# Color logging output
logging.addLevelName(logging.INFO, "\033[0;32m%s\033[0;0m" % "INFO")
logging.addLevelName(logging.WARNING, "\033[0;33m%s\033[0;0m" % "WARNING")
logging.addLevelName(logging.ERROR, "\033[0;31m%s\033[0;0m" % "ERROR")

def copy_file(src, dst):
    subprocess.run(CP + [src, dst], check=True)

def make_path(dir, prefix, suffix):
    return os.path.join(dir, f"{prefix}{suffix}")

def truncate_str(s, n):
    assert n >= 3
    if len(s) <= n:
        return s
    else:
        return s[:(n-3)] + "..."

# ===== Crate lists and cargo download =====

def count_lines(cratefile, header_row=True):
    with open(cratefile, 'r') as fh:
        result = len(fh.readlines())
        if header_row:
            result -= 1
        return result

def get_crate_names(cratefile):
    crates = []
    with open(cratefile, newline='') as infile:
        in_reader = csv.reader(infile, delimiter=',')
        for i, row in enumerate(in_reader):
            if i > 0:
                logging.debug(f"Input crate: {row[0]} ({','.join(row[1:])})")
                crates.append(row[0])
    return crates

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

def sort_summary_dict(d):
    return sorted(d.items(), key=lambda x: x[1], reverse=True)

def make_pattern_summary(pattern_summary):
    result = ""
    result += "===== Patterns =====\n"
    result += "Total instances of each effect pattern:\n"
    pattern_sorted = sort_summary_dict(pattern_summary)
    for p, n in pattern_sorted:
        result += f"{p}: {n}\n"
    return result

def make_crate_summary(crate_summary):
    result = ""
    result += "===== Crate Summary =====\n"
    result += "Number of effects by crate:\n"
    crate_sorted = sort_summary_dict(crate_summary)
    num_nonzero = 0
    num_zero = 0
    for c, n in crate_sorted:
        if n > 0:
            num_nonzero += 1
            result += f"{c}: {n}\n"
        else:
            num_zero += 1
    result += "===== Crate Totals =====\n"
    result += f"{num_nonzero} crates with 1 or more effects\n"
    result += f"{num_zero} crates with 0 effects\n"

    return result

# ===== Syn backend =====

def scan_crate(crate, crate_dir, of_interest, add_args):
    logging.debug(f"Scanning crate: {crate}")
    command = CARGO_SCAN + [crate_dir] + add_args
    logging.debug(f"Running: {command}")
    proc = subprocess.Popen(command, stdout=subprocess.PIPE)

    # read CSV lines, skipping header row
    first_line = True
    for line in iter(proc.stdout.readline, b""):
        effect_csv = line.strip().decode("utf-8")
        if first_line:
            assert effect_csv == CARGO_SCAN_CSV_HEADER
            first_line = False
        else:
            effect_pat = effect_csv.split(", ")[3]
            yield effect_pat, effect_csv

# ===== Entrypoint =====

def main():
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument('-c', '--crate', help="Crate name to scan")
    group.add_argument('-i', '--infile', help="Instead of scanning a single crate, provide a list of crates as a CSV file")
    parser.add_argument('-t', '--test-run', action="store_true", help=f"Test run: use existing crates in {TEST_CRATES_DIR} instead of downloading via cargo-download")
    parser.add_argument('-o', '--output-prefix', help="Output file prefix to save results")
    parser.add_argument('-s', '--std', action="store_true", help="Flag standard library imports only")
    parser.add_argument('-v', '--verbose', action="count", help="Verbosity level: v=err, vv=warning, vvv=info, vvvv=debug, vvvvv=trace (default: info)", default=0)

    args = parser.parse_args()

    if args.verbose > 5:
        logging.error("verbosity only goes up to 4 (-vvvv)")
        sys.exit(1)
    log_level = [logging.INFO, logging.ERROR, logging.WARNING, logging.INFO, logging.DEBUG][args.verbose]
    logging.basicConfig(level=log_level)
    logging.debug(args)

    if args.test_run:
        logging.info("=== Test run ===")
        crates_dir = TEST_CRATES_DIR
    else:
        crates_dir = CRATES_DIR

    if args.infile is None:
        num_crates = 1
        crates = [args.crate]
        crates_infostr = f"{args.crate}"
    else:
        num_crates = count_lines(args.infile)
        crates = get_crate_names(args.infile)
        crates_infostr = f"{num_crates} crates from {args.infile}"

    if args.output_prefix is None and num_crates > 1:
        logging.warning("No results prefix specified; results of this run will not be saved")

    of_interest = OF_INTEREST_STD
    if not args.std:
        of_interest += OF_INTEREST_OTHER

    add_args = []
    if args.verbose >= 4:
        add_args = ["-v"]

    logging.info(f"=== Scanning {crates_infostr} in {crates_dir} ===")

    results = []
    crate_summary = {c: 0 for c in crates}
    pattern_summary = {p: 0 for p in of_interest}
    progress_inc = num_crates // PROGRESS_INCS

    for i, crate in enumerate(crates):
        if progress_inc > 0 and i > 0 and i % progress_inc == 0:
            progress = 100 * i // num_crates
            logging.info(f"{progress}% complete")

        try:
            download_crate(crates_dir, crate, args.test_run)
        except subprocess.CalledProcessError as e:
            logging.error(f"cargo-download failed for crate: {crate} ({e})")
            sys.exit(1)

        crate_dir = os.path.join(crates_dir, crate)
        for eff_pat, eff_csv in scan_crate(crate, crate_dir, of_interest, add_args):
            logging.debug(f"effect found: {eff_csv}")
            results.append(eff_csv)
            # Update summaries
            crate_summary[crate] += 1
            pattern_summary.setdefault(eff_pat, 0)
            pattern_summary[eff_pat] += 1

    # Sanity check
    if sum(crate_summary.values()) != sum(pattern_summary.values()):
        logging.error("Logic error: crate summary and pattern summary were inconsistent!")

    logging.info("=== Results ===")

    if args.output_prefix is None:
        if num_crates == 1:
            results_str = "===== All results =====\n" + '\n'.join(results)
            logging.info(results_str)

        else:
            logging.info(make_pattern_summary(pattern_summary).rstrip())

        logging.info(make_crate_summary(crate_summary))
    else:
        logging.info(f"=== Saving results ===")

        prefix = args.output_prefix
        results_path = make_path(RESULTS_DIR, prefix, RESULTS_ALL_SUFFIX)
        pattern_path = make_path(RESULTS_DIR, prefix, RESULTS_PATTERN_SUFFIX)
        summary_path = make_path(RESULTS_DIR, prefix, RESULTS_SUMMARY_SUFFIX)

        pat_str = make_pattern_summary(pattern_summary)
        crate_str = make_crate_summary(crate_summary)

        logging.info(f"Saving all results to {results_path}")
        with open(results_path, 'w') as fh:
            fh.write(CARGO_SCAN_CSV_HEADER + '\n')
            for eff_csv in results:
                fh.write(eff_csv + '\n')

        logging.info(f"Saving pattern totals to {pattern_path}")
        with open(pattern_path, 'w') as fh:
            fh.write(pat_str)

        logging.info(f"Saving summary to {summary_path}")
        with open(summary_path, 'w') as fh:
            fh.write(crate_str)

if __name__ == "__main__":
    main()
