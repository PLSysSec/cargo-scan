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

CSV_HEADER = "crate, pattern of interest, directory, file, use line\n"

# Top 200 most downloaded crates
TOP_200_CRATES = [
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
    "mio",
    "ansi_term",
    "idna",
    "indexmap",
    "ppv-lite86",
    "pin-project-lite",
    "unicode-width",
    "either",
    "tokio",
    "itertools",
    "slab",
    "futures",
    "unicode-normalization",
    "rustc_version",
    "chrono",
    "memoffset",
    "fnv",
    "env_logger",
    "typenum",
    "unicode-bidi",
    "heck",
    "pkg-config",
    "winapi",
    "matches",
    "hyper",
    "crossbeam-epoch",
    "miniz_oxide",
    "thread_local",
    "thiserror",
    "thiserror-impl",
    "termcolor",
    "toml",
    "opaque-debug",
    "anyhow",
    "futures-core",
    "socket2",
    "crossbeam-channel",
    "arrayvec",
    "futures-util",
    "http",
    "futures-task",
    "tokio-util",
    "futures-channel",
    "futures-sink",
    "unicode-segmentation",
    "crossbeam-deque",
    "nom",
    "httparse",
    "h2",
    "vec_map",
    "futures-io",
    "semver-parser",
    "proc-macro-hack",
    "pin-project",
    "humantime",
    "pin-project-internal",
    "backtrace",
    "tracing",
    "pin-utils",
    "tinyvec",
    "crc32fast",
    "tracing-core",
    "sha2",
    "instant",
    "rustc-demangle",
    "nix",
    "remove_dir_all",
    "http-body",
    "tempfile",
    "futures-macro",
    "mime",
    "quick-error",
    "hex",
    "rand_hc",
    "futures-executor",
    "uuid",
    "want",
    "openssl-sys",
    "adler",
    "sha-1",
    "serde_urlencoded",
    "flate2",
    "walkdir",
    "same-file",
    "try-lock",
    "object",
    "form_urlencoded",
    "tokio-macros",
    "glob",
    "num-bigint",
    "proc-macro-error",
    "wasi",
    "openssl",
    "tower-service",
    "proc-macro-error-attr",
    "encoding_rs",
    "linked-hash-map",
    "tinyvec_macros",
    "ahash",
    "rayon",
    "gimli",
    "unicase",
    "async-trait",
    "openssl-probe",
    "spin",
    "rayon-core",
    "reqwest",
    "synstructure",
    "signal-hook-registry",
    "foreign-types",
    "redox_syscall",
    "addr2line",
    "httpdate",
    "foreign-types-shared",
    "subtle",
    "hmac",
    "crypto-mac",
    "which",
    "regex-automata",
    "native-tls",
    "tracing-attributes",
    "rand_pcg",
    "winapi-x86_64-pc-windows-gnu",
    "paste",
    "dirs",
    "winapi-i686-pc-windows-gnu",
    "static_assertions",
    "bstr",
    "block-padding",
    "net2",
    "cpufeatures",
    "hyper-tls",
    "dtoa",
    "num-rational",
    "iovec",
    "crossbeam-queue",
    "rustls",
    "ring",
    "fixedbitset",
    "ipnet",
    "untrusted",
    "petgraph",
    "miow",
    "libloading",
    "proc-macro-nested",
    "time-macros",
    "yaml-rust",
    "sct",
    "webpki",
    "stable_deref_trait",
]
assert len(TOP_200_CRATES) == 200

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
        fh.write(CSV_HEADER)
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
for crate in TOP_200_CRATES:
    download_crate(crate)
    scan_crate(crate, results)
save_results(results)
