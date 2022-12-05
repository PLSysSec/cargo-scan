#!/usr/bin/env python3

"""
Cargo Scan

Script to download Cargo crate source code and analyze module-level imports.
"""

import argparse
import csv
import logging
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from functools import partial, partialmethod

# ===== Check requirements =====

# requires v3.7 for dataclasses
MIN_PYTHON = (3, 7)
if sys.version_info < MIN_PYTHON:
    version = f"{MIN_PYTHON[0]}.{MIN_PYTHON[1]}"
    found = f"{sys.version_info.major}.{sys.version_info.minor}"
    sys.exit(f"Error: Python {version} or later is required (found {found}).")

def check_installed(args, check_exit_code=True):
    try:
        subprocess.run(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=check_exit_code)
    except Exception as e:
        sys.exit(f"missing dependency: command {args} failed ({e})")

check_installed(["rustc", "--version"])
check_installed(["cargo", "download", "--version"], check_exit_code=False)
check_installed(["cargo", "mirai", "--version"])

# ===== Constants =====

# Number of progress tracking messages to display
PROGRESS_INCS = 5

CRATES_DIR = "data/packages"
TEST_CRATES_DIR = "data/test-packages"
RUST_SRC = "src"

SYN_DEBUG = "./rust-src/target/debug/find_calls"
SYN_RELEASE = "./rust-src/target/release/find_calls"

MIRAI_CONFIG = "mirai/config.json"
MIRAI_FLAGS_KEY = "MIRAI_FLAGS"
MIRAI_FLAGS_VAL = f"--call_graph_config ../../../{MIRAI_CONFIG}"

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

# Set up trace logging level below debug
# https://stackoverflow.com/a/55276759/2038713
logging.TRACE = logging.DEBUG - 5
logging.addLevelName(logging.TRACE, 'TRACE')
logging.Logger.trace = partialmethod(logging.Logger.log, logging.TRACE)
logging.trace = partial(logging.log, logging.TRACE)

# Color logging output
logging.addLevelName(logging.INFO, "\033[0;32m%s\033[0;0m" % "INFO")
logging.addLevelName(logging.WARNING, "\033[0;33m%s\033[0;0m" % "WARNING")
logging.addLevelName(logging.ERROR, "\033[0;31m%s\033[0;0m" % "ERROR")

def copy_file(src, dst):
    subprocess.run(["cp", src, dst], check=True)

def make_path(dir, prefix, suffix):
    return os.path.join(dir, f"{prefix}{suffix}")

def truncate_str(s, n):
    assert n >= 3
    if len(s) <= n:
        return s
    else:
        return s[:(n-3)] + "..."

# ===== CSV output for effects =====

def sanitize_comma(s):
    if "," in s:
        logging.warning(f"found unexpected comma in: {s}")
    return s.replace(',', '')

@dataclass
class Effect:
    """
    Data related to an effect
    used as an intermediate output for both the grep-based and the
    mirai-based effects analysis
    """
    # Name of crate, e.g. num_cpus
    crate: str
    # Full path to module, e.g. num_cpus::linux
    module: str
    # Caller function, e.g. logical_cpus
    caller: str
    # Callee (effect) function, e.g. libc::sched_getaffinity
    callee: str
    # Effect pattern -- prefix of callee (effect), e.g. libc
    pattern: str
    # Directory in which the call occurs
    dir: str
    # File in which the call occurs -- in the above directory
    file: str
    # Loc in which the call occurs -- in the above file
    loc: str

    def csv_header():
        return ", ".join(["crate", "module", "caller", "callee", "pattern", "dir", "file", "loc"])

    def to_csv(self):
        crate = sanitize_comma(self.crate)
        module = sanitize_comma(self.module)
        caller = sanitize_comma(self.caller)
        callee = sanitize_comma(self.callee)
        pattern = sanitize_comma(self.pattern)
        dir = sanitize_comma(self.dir)
        file = sanitize_comma(self.file)
        loc = sanitize_comma(self.loc)

        return ", ".join([crate, module, caller, callee, pattern, dir, file, loc])

# ===== Used by both backends =====

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
                logging.trace(f"Input crate: {row[0]} ({','.join(row[1:])})")
                crates.append(row[0])
    return crates

def download_crate(crates_dir, crate, test_run):
    target = os.path.join(crates_dir, crate)
    if os.path.exists(target):
        logging.trace(f"Found existing crate: {target}")
    else:
        if test_run:
            logging.warning(f"Crate not found during test run: {target}")
        else:
            logging.info(f"Downloading crate: {target}")
            subprocess.run(["cargo", "download", "-x", crate, "-o", target], check=True)

def sort_summary_dict(d):
    return sorted(d.items(), key=lambda x: x[1], reverse=True)

def make_pattern_summary(pattern_summary):
    result = ""
    result += "===== Patterns =====\n"
    result += "Total instances of each import pattern:\n"
    pattern_sorted = sort_summary_dict(pattern_summary)
    for p, n in pattern_sorted:
        result += f"{p}: {n}\n"
    return result

def make_crate_summary(crate_summary):
    result = ""
    result += "===== Crate Summary =====\n"
    result += "Number of dangerous imports by crate:\n"
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
    result += f"{num_nonzero} crates with 1 or more dangerous imports\n"
    result += f"{num_zero} crates with 0 dangerous imports\n"

    return result

def is_of_interest(line, of_interest):
    found = None
    for p in of_interest:
        if re.search(p, line):
            if found is not None:
                logging.warning(f"Matched multiple patterns of interest: {line}")
            found = p
    return found

# ===== Syn backend =====

def scan_file(crate, root, file, of_interest):
    filepath = os.path.join(root, file)
    logging.trace(f"Scanning file: {filepath}")

    logging.debug(f"Running: {[SYN_DEBUG, filepath]}")
    proc = subprocess.Popen([SYN_DEBUG, filepath], stdout=subprocess.PIPE)
    for line in iter(proc.stdout.readline, b""):
        line = line.strip().decode("utf-8")
        eff = Effect(*line.split(", "))

        # syn backend returns raw call sites, without a matched pattern
        assert eff.pattern == "[none]"
        eff.pattern = is_of_interest(eff.callee, of_interest)
        if eff.pattern is None:
            logging.trace(f"Skipping: {eff}")
        else:
            logging.trace(f"Of interest: {eff}")
            yield eff

def scan_crate(crate, crate_dir, of_interest):
    logging.debug(f"Scanning crate: {crate}")
    src = os.path.join(crate_dir, RUST_SRC)
    for root, dirs, files in os.walk(src):
        # Hack to make os.walk work in alphabetical order
        # https://stackoverflow.com/questions/6670029/can-i-force-os-walk-to-visit-directories-in-alphabetical-order
        # This is fragile. It relies on modifying dirs.sort() in place, and
        # doesn't work if topdown=False is set.
        files.sort()
        dirs.sort()
        for file in files:
            if os.path.splitext(file)[1] == ".rs":
                yield from scan_file(crate, root, file, of_interest)

# ===== MIRAI backend =====

def parse_mirai_call_line(line):
    parts = (line
        .replace(" (", " ")
        .replace("(", " ")
        .replace(" ~ ", " ")
        .replace("), ", " ")
        .replace(")", " ")
        .replace(": ", " ")
        .strip()
        .split(" ")
    )
    if len(parts) != 6:
        logging.warning(f"MIRAI output: expected 6 parts: {parts}")
        return None
    # Examples:
    # ['DefId', '0:6', 'num_cpus[1818]::get_num_physical_cpus', 'src/lib.rs:324:20', '324:34', '#0']
    # ['DefId', '0:5', 'num_cpus[1818]::get_physical', 'src/lib.rs:109:5', '109:28', '#0']
    fun = re.sub(r"\[[0-9a-f]*\]", "", parts[2])
    module = fun.rsplit("::", 1)[0]
    src_dir, path = tuple(parts[3].split("/"))
    file, loc = tuple(path.split(':', 1))
    return module, fun, src_dir, file, loc

def mirai_call_path_as_effect(crate, crate_dir, call_path, of_interest):
    # Convert a call path to an Effect object
    # crate is the name of the crate
    # crate_dir is the path to the crate (from scan.py top-level directory)
    # call_path is a nonempty list of (effect_fun, src_dir, path)
    # of_interest is a list of patterns
    callee_mod, callee_fun, src_dir, callee_file, callee_loc = call_path[0]
    if len(call_path) > 1:
        caller_mod, caller_fun, src_dir2, caller_file, caller_loc = call_path[1]
        if src_dir != src_dir2:
            logging.warning(f"MIRAI: callee and caller in different source dirs: {src_dir1} and {src_dir2}")
    else:
        logging.warning(f"MIRAI: call path of length 1: {call_path}")
        caller = "Unknown"
        caller_path = "Unknown"

    dir = crate_dir + "/" + src_dir

    pattern = is_of_interest(callee_fun, of_interest)
    if pattern is None:
        logging.warning("MIRAI: output didn't match any pattern of interest")
        pattern = "::".join(callee_mod.split("::")[0:2])

    return Effect(
        crate,
        caller_mod,
        caller_fun,
        callee_fun,
        pattern,
        dir,
        callee_file,
        callee_loc,
    )

def scan_crate_mirai(crate, crate_dir, of_interest):
    # Run our MIRAI fork; yield effects
    os.environ[MIRAI_FLAGS_KEY] = MIRAI_FLAGS_VAL
    subprocess.run(["cargo", "clean"], cwd=crate_dir, check=True)
    proc = subprocess.Popen(["cargo", "mirai"], cwd=crate_dir, stderr=subprocess.DEVNULL, stdout=subprocess.PIPE)
    call_path = []
    for line in iter(proc.stdout.readline, b""):
        line = line.strip().decode("utf-8")
        if line == "~~~New Fn~~~~~":
            logging.trace("MIRAI: new function")
        elif line == "Call Path:":
            logging.trace("MIRAI: new call path")
            if call_path:
                yield mirai_call_path_as_effect(crate, crate_dir, call_path, of_interest)
            call_path = []
        elif line[0:6] == "Call: ":
            result = parse_mirai_call_line(line[6:])
            if result is not None:
                logging.trace(f"MIRAI call: {result}")
                call_path.append(result)
        else:
            logging.warning(f"Unrecognized MIRAI output line: {line}")
    if call_path:
        yield mirai_call_path_as_effect(crate, crate_dir, call_path, of_interest)

def view_callgraph_mirai(crate_dir):
    subprocess.run(["dot", "-Tpng", "graph.dot", "-o", "graph.png"], cwd=crate_dir, check=True)
    subprocess.run(["open", "graph.png"], cwd=crate_dir, check=True)

# ===== Entrypoint =====

def main():
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument('-c', '--crate', help="Crate name to scan")
    group.add_argument('-i', '--infile', help="Instead of scanning a single crate, provide a list of crates as a CSV file")
    parser.add_argument('-t', '--test-run', action="store_true", help=f"Test run: use existing crates in {TEST_CRATES_DIR} instead of downloading via cargo-download")
    parser.add_argument('-o', '--output-prefix', help="Output file prefix to save results")
    parser.add_argument('-m', '--mirai', action="store_true", help="Use MIRAI to scan packages instead of pattern matching")
    parser.add_argument('-g', '--call-graph', action="store_true", help="View the call graph as a .png (only works with -m; requires graphviz to be installed)")
    parser.add_argument('-s', '--std', action="store_true", help="Flag standard library imports only")
    parser.add_argument('-v', '--verbose', action="count", help="Verbosity level: v=err, vv=warning, vvv=info, vvvv=debug, vvvvv=trace (default: info)", default=0)

    args = parser.parse_args()

    if args.verbose > 5:
        logging.error("verbosity only goes up to 5 (-vvvvv)")
        sys.exit(1)
    log_level = [logging.INFO, logging.ERROR, logging.WARNING, logging.INFO, logging.DEBUG, logging.TRACE][args.verbose]
    logging.basicConfig(level=log_level)
    logging.debug(args)

    if args.call_graph and not args.mirai:
        logging.warning("-g/--call-graph option ignored without -m/--mirai")

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

    progress_inc = num_crates // PROGRESS_INCS
    of_interest = OF_INTEREST_STD
    if not args.std:
        of_interest += OF_INTEREST_OTHER

    if args.mirai:
        scan_fun = scan_crate_mirai
    else:
        scan_fun = scan_crate

    logging.info(f"=== Scanning {crates_infostr} in {crates_dir} ===")

    results = []
    crate_summary = {c: 0 for c in crates}
    pattern_summary = {p: 0 for p in of_interest}

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
        for effect in scan_fun(crate, crate_dir, of_interest):
            logging.debug(f"effect found: {effect.to_csv()}")
            results.append(effect)
            # Update summaries
            crate_summary[crate] += 1
            pattern_summary.setdefault(effect.pattern, 0)
            pattern_summary[effect.pattern] += 1

    if args.mirai and args.call_graph:
        logging.info("=== Generating call graph as a PNG ===")
        view_callgraph_mirai(crate_dir)

    # Sanity check
    if sum(crate_summary.values()) != sum(pattern_summary.values()):
        logging.error("Logic error: crate summary and pattern summary were inconsistent!")

    if args.output_prefix is None:
        results_str = "=== Results ===\n"
        if num_crates == 1:
            for result in results:
                results_str += result.to_csv()
                results_str += '\n'
        results_str += make_crate_summary(crate_summary)
        logging.info(results_str)
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
            fh.write(Effect.csv_header() + '\n')
            for effect in results:
                fh.write(effect.to_csv() + '\n')

        logging.info(f"Saving pattern totals to {pattern_path}")
        with open(pattern_path, 'w') as fh:
            fh.write(pat_str)

        logging.info(f"Saving summary to {summary_path}")
        with open(summary_path, 'w') as fh:
            fh.write(crate_str)

if __name__ == "__main__":
    main()
