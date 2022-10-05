"""
Script to download Cargo crate source code and analyze module-level imports.
"""

import csv
import logging
import os
import re
import subprocess
import sys
from functools import partial, partialmethod

# ===== Input arguments =====
# These should be CLI args but I'm lazy

# Change to true to do a test run on dummy packages
TEST_RUN = False

# Number of top crates to analyze
# (ignored for a test run)
USE_TOP = 200

# ===== Logging config =====

# Set up trace logging level below debug
# https://stackoverflow.com/a/55276759/2038713
logging.TRACE = logging.DEBUG - 5
logging.addLevelName(logging.TRACE, 'TRACE')
logging.Logger.trace = partialmethod(logging.Logger.log, logging.TRACE)
logging.trace = partial(logging.log, logging.TRACE)

# Logging level
logging.basicConfig(level=logging.INFO)

# ===== Constants =====

# For progress tracking purposes
PROGRESS_INCS = 5
PROGRESS_INC = USE_TOP // PROGRESS_INCS

RESULTS_DIR = "experiments/results"
RESULTS_ALL_SUFFIX = "all.csv"
RESULTS_SUMMARY_SUFFIX = "summary.txt"

CRATES_DIR = "experiments/packages"
SRC_DIR = "src"
TEST_CRATES_DIR = "experiments/test-packages"
TEST_CRATES = [ "dummy" ]

TOP_CRATES_CSV = "data/crates.csv"

OF_INTEREST = [
    "std::env",
    "std::fs",
    "std::net",
    "std::os",
    "std::path",
    "std::process",
]

CSV_HEADER = "crate, pattern of interest, directory, file, use line\n"

# ===== Utility =====

def copy_file(src, dst):
    subprocess.run(["cp", src, dst])

def truncate_str(s, n):
    assert n >= 3
    if len(s) <= n:
        return s
    else:
        return s[:(n-3)] + "..."

# ===== Main script =====

def get_top_crates(n):
    with open(TOP_CRATES_CSV, newline='') as infile:
        in_reader = csv.reader(infile, delimiter=',')
        crates = []
        for i, row in enumerate(in_reader):
            if i > 0:
                logging.trace(f"Top crate: {row[0]} ({row[1]} downloads)")
                crates.append(row[0])
            if i == n:
                assert len(crates) == n
                return crates
    logging.error(f"Not enough crates. Asked for {n}, found {len(crates)}")
    sys.exit(1)

def download_crate(crates_dir, crate):
    target = os.path.join(crates_dir, crate)
    if os.path.exists(target):
        logging.trace(f"Found existing crate: {target}")
    else:
        if TEST_RUN:
            logging.warning(f"Crate not found during test run: {target}")
        else:
            logging.info(f"Downloading crate: {target}")
            subprocess.run(["cargo", "download", "-x", crate, "-o", target])

def save_results(results, results_prefix):
    results_file = f"{results_prefix}_{RESULTS_ALL_SUFFIX}"
    results_path = os.path.join(RESULTS_DIR, results_file)
    logging.info(f"Saving raw results to {results_path}")
    with open(results_path, 'w') as fh:
        fh.write(CSV_HEADER)
        for line in results:
            fh.write(line + '\n')

def sort_summary_dict(d):
    return sorted(d.items(), key=lambda x: x[1], reverse=True)

def save_summary(crate_summary, pattern_summary, results_prefix):
    results_file = f"{results_prefix}_{RESULTS_SUMMARY_SUFFIX}"
    results_path = os.path.join(RESULTS_DIR, results_file)

    # Sanity check
    assert sum(crate_summary.values()) == sum(pattern_summary.values())

    logging.info(f"Saving summary to {results_path}")
    with open(results_path, 'w') as fh:
        fh.write("===== Patterns =====\n")
        fh.write("Total instances of each import pattern:\n")
        pattern_sorted = sort_summary_dict(pattern_summary)
        for p, n in pattern_sorted:
            fh.write(f"{p}: {n}\n")

        fh.write("===== Crate Summary =====\n")
        fh.write("Number of dangerous imports by crate:\n")
        crate_sorted = sort_summary_dict(crate_summary)
        num_nonzero = 0
        num_zero = 0
        for c, n in crate_sorted:
            if n > 0:
                num_nonzero += 1
                fh.write(f"{c}: {n}\n")
            else:
                num_zero += 1
        fh.write("===== Crate Totals =====\n")
        fh.write(f"{num_nonzero} crates with 1 or more dangerous imports\n")
        fh.write(f"{num_zero} crates with 0 dangerous imports\n")

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

def parse_use(expr):
    """
    Heuristically parse a use ...; expression, returning a list of crate
    imports.

    This function is hacky and best-effort (it prints warnings if
    it detects anything it doesn't recognize).
    Most of the logic is just dealing with {} replacements.

    The input should start with 'use ' and end in a newline.
    """
    smry = truncate_str(expr, 100)

    # Preconditions
    if expr[-1] != '\n':
        logging.warning(f"Expected newline-terminated use expression: {smry}")
        return []
    elif expr[0:4] != "use ":
        logging.warning(f"Expected 'use' expression: {smry}")
        return []
    elif ";" not in expr:
        logging.warning(f"Expected semicolon in: {smry}")
        return []
    elif '/' in expr:
        logging.warning(f"Unexpected extra slash in: {smry}")
        return []

    # Remove 'use ' at the beginning and newlines
    expr = expr[4:]
    expr = expr.replace('\n', '')

    # Remove semicolon at the end
    if expr[-1] != ';':
        logging.warning(f"Expected ; at end of use expression: {smry}")
        return []
    expr = expr[:-1]
    if ';' in expr:
        logging.warning(f"Unexpected extra semicolon in: {smry}")

    # Replace {} instances, iteratively
    to_search = [expr]
    done = []
    while to_search:
        l = to_search.pop()
        if m := re.fullmatch("(.*){([^{}]*)}(.*)", l):
            for mid in m[2].split(','):
                to_search.append(m[1] + mid.strip() + m[3])
        elif '{' in l:
            logging.warning(f"Unexpected extra {{ in expr: {smry}")
        elif '}' in l:
            logging.warning(f"Unexpected extra }} in expr: {smry}")
        else:
            done.append(l)

    # Dedup and sort final results
    return sorted(set(done))

def sanitize_comma(s):
    if "," in s:
        logging.warning(f"found unexpected comma in: {s}")
    return s.replace(',', '')

def to_csv(crate, pat, root, file, use_expr):
    crate = sanitize_comma(crate)
    pat = sanitize_comma(pat)
    root = sanitize_comma(root)
    file = sanitize_comma(file)
    use_expr = sanitize_comma(use_expr)
    return f"{crate}, {pat}, {root}, {file}, {use_expr}"

def scan_use(crate, root, file, use_expr):
    """
    Scan a single use ...; expression.
    Return a list of pairs of a pattern and the CSV output.

    Calls parse_use to parse the Rust syntax.
    """
    results = []
    for use in parse_use(use_expr):
        pat = of_interest(use)
        if pat is None:
            logging.trace(f"Skipping: {use}")
        else:
            logging.trace(f"Of interest: {use}")
            results.append((pat, to_csv(crate, pat, root, file, use)))
    return results

def scan_rs(fh):
    """
    Scan a rust file handle until at least one semicolon is found.
    Ignore comments.

    Yield (possibly multi-line) newline-terminated strings.
    """
    curr = ""
    for line in fh:
        if line is None:
            return
        curr += line
        if ';' in curr:
            curr = re.sub("[ ]*//.*\n", "\n", curr)
            curr = re.sub("/\*.*\*/", "", curr, flags=re.DOTALL)
            curr = re.sub("/\*.*$", "", curr, flags=re.DOTALL)
            curr = re.sub("^.*\*/", "", curr, flags=re.DOTALL)
            yield curr
            curr = ""

def scan_file(crate, root, file, results, crate_summary, pattern_summary):
    filepath = os.path.join(root, file)
    logging.trace(f"Scanning file: {filepath}")
    with open(filepath) as fh:
        scanner = scan_rs(fh)
        for expr in scanner:
            if m := re.fullmatch(".*(^use .*\n)", expr, flags=re.MULTILINE | re.DOTALL):
                # Scan use expression
                for pat, result in scan_use(crate, root, file, m[1]):
                    results.append(result)
                    # Update summaries
                    crate_summary[crate] += 1
                    pattern_summary[pat] += 1

def scan_crate(crate, crate_dir, results, crate_summary, pattern_summary):
    logging.debug(f"Scanning crate: {crate}")
    src = os.path.join(crate_dir, crate, SRC_DIR)
    for root, dirs, files in os.walk(src):
        # Hack to make os.walk work in alphabetical order
        # https://stackoverflow.com/questions/6670029/can-i-force-os-walk-to-visit-directories-in-alphabetical-order
        # This is fragile. It relies on modifying dirs.sort() in place, and
        # doesn't work if topdown=False is set.
        files.sort()
        dirs.sort()
        for file in files:
            if os.path.splitext(file)[1] == ".rs":
                scan_file(
                    crate,
                    root,
                    file,
                    results,
                    crate_summary,
                    pattern_summary,
                )

# ===== Entrypoint =====

if TEST_RUN:
    logging.info(f"==== Test run: scanning crates in {TEST_CRATES_DIR} =====")
    crates_dir = TEST_CRATES_DIR
    crates = TEST_CRATES
    results_prefix = "test"
else:
    logging.info(f"==== Scanning the top {USE_TOP} crates =====")
    crates_dir = CRATES_DIR
    crates = get_top_crates(USE_TOP)
    results_prefix = f"top{USE_TOP}"

results = []
crate_summary = {c: 0 for c in crates}
pattern_summary= {p: 0 for p in OF_INTEREST}

for i, crate in enumerate(crates):
    if i > 0 and i % PROGRESS_INC == 0:
        progress = 100 * i // USE_TOP
        logging.info(f"{progress}% complete")
    download_crate(crates_dir, crate)
    scan_crate(crate, crates_dir, results, crate_summary, pattern_summary)
logging.info(f"===== Results =====")
# TODO: display results, don't just save them
save_results(results, results_prefix)
save_summary(crate_summary, pattern_summary, results_prefix)
