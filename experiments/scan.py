"""
Script to download Cargo crate source code and analyze module-level imports.
"""

import argparse
import csv
import logging
import os
import re
import subprocess
import sys
from functools import partial, partialmethod

# ===== Constants =====

# Number of progress tracking messages to display
PROGRESS_INCS = 5

RESULTS_DIR = "data/results"
RESULTS_ALL_SUFFIX = "all.csv"
RESULTS_SUMMARY_SUFFIX = "summary.txt"

CRATES_DIR = "data/packages"
SRC_DIR = "src"
TEST_CRATES_DIR = "data/test-packages"
TEST_CRATES = [ "dummy" ]

TOP_CRATES_CSV = "data/crates.csv"

# Potentially dangerous stdlib imports
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

# Set up trace logging level below debug
# https://stackoverflow.com/a/55276759/2038713
logging.TRACE = logging.DEBUG - 5
logging.addLevelName(logging.TRACE, 'TRACE')
logging.Logger.trace = partialmethod(logging.Logger.log, logging.TRACE)
logging.trace = partial(logging.log, logging.TRACE)

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

def download_crate(crates_dir, crate, test_run):
    target = os.path.join(crates_dir, crate)
    if os.path.exists(target):
        logging.trace(f"Found existing crate: {target}")
    else:
        if test_run:
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
                logging.warning(f"Matched multiple patterns of interest: {line}")
            found = p
    return found

def parse_use_core(expr, smry):
    stack = []
    cur = ""
    pending = ""
    for ch in expr:
        if ch in '{,}':
            cur, pending = cur + pending.strip(), ""
        if ch == '{':
            stack.append(cur)
        elif ch == ',':
            if not stack:
                logging.warning(f"unexpected ,: {smry}")
                return
            yield cur
            cur = stack[-1]
        elif ch == '}':
            if not stack:
                logging.warning(f"unexpected }}: {smry}")
                return
            stack.pop()
        else:
            pending += ch
    if stack:
        logging.warning(f"unclosed {{: {smry}")
    cur, pending = cur + pending.strip(), ""
    yield cur

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

    # Sort final results
    return sorted(list(parse_use_core(expr, smry)))

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
    for use in parse_use(use_expr):
        pat = of_interest(use)
        if pat is None:
            logging.trace(f"Skipping: {use}")
        else:
            logging.trace(f"Of interest: {use}")
            yield (pat, to_csv(crate, pat, root, file, use))

def scan_rs(fh):
    """
    Scan a rust file handle until at least one semicolon is found.
    Ignore comments.

    Yield (possibly multi-line) newline-terminated strings.
    """
    curr = ""
    for line in fh:
        curr += line
        if ';' in curr:
            curr = re.sub("[ ]*//.*\n", "\n", curr)
            curr = re.sub("/\*.*\*/", "", curr, flags=re.DOTALL)
            curr = re.sub("/\*.*$", "", curr, flags=re.DOTALL)
            curr = re.sub("^.*\*/", "", curr, flags=re.DOTALL)
            yield curr
            curr = ""

def scan_file(crate, root, file):
    filepath = os.path.join(root, file)
    logging.trace(f"Scanning file: {filepath}")
    with open(filepath) as fh:
        scanner = scan_rs(fh)
        for expr in scanner:
            if m := re.fullmatch(".*^(pub )?(use .*\n)", expr, flags=re.MULTILINE | re.DOTALL):
                # Scan use expression
                yield from scan_use(crate, root, file, m[2])

def scan_crate(crate, crate_dir):
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
                yield from scan_file(crate, root, file)

# ===== Entrypoint =====

if __name__ == "__main__":

    parser = argparse.ArgumentParser()
    parser.add_argument('num_crates', nargs='?', help="Number of top crates to analyze (ignored for a test run)", default=100)
    parser.add_argument('-t', '--test', action="store_true", help="Test run on dummy packages")
    parser.add_argument('-v', '--verbose', action="count", help="Verbosity level: v=err, vv=warning, vvv=info, vvvv=debug, vvvvv=trace (default: info)", default=0)

    args = vars(parser.parse_args())

    test_run = args["test"]
    log_level = [logging.INFO, logging.ERROR, logging.WARNING, logging.INFO, logging.DEBUG, logging.TRACE][args["verbose"]]
    num_crates = int(args["num_crates"])

    logging.basicConfig(level=log_level)
    logging.debug(args)

    if test_run:
        num_crates = len(TEST_CRATES)
        crates_dir = TEST_CRATES_DIR
        logging.info(f"===== Test run: scanning {num_crates} crate(s) in {crates_dir} =====")
        crates = TEST_CRATES
        results_prefix = "test"
    else:
        crates_dir = CRATES_DIR
        logging.info(f"===== Scanning the top {num_crates} crates in {crates_dir} =====")
        crates = get_top_crates(num_crates)
        results_prefix = f"top{num_crates}"

    progress_inc = num_crates // PROGRESS_INCS

    results = []
    crate_summary = {c: 0 for c in crates}
    pattern_summary= {p: 0 for p in OF_INTEREST}

    for i, crate in enumerate(crates):
        if i > 0 and i % progress_inc == 0:
            progress = 100 * i // num_crates
            logging.info(f"{progress}% complete")

        download_crate(crates_dir, crate, test_run)

        for pat, result in scan_crate(crate, crates_dir):
            results.append(result)
            # Update summaries
            crate_summary[crate] += 1
            pattern_summary[pat] += 1

    logging.info(f"===== Results =====")
    # TODO: display results, don't just save them
    save_results(results, results_prefix)
    save_summary(crate_summary, pattern_summary, results_prefix)
