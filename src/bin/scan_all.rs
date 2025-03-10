//! The scan_all binary: Scan a set of crates in parallel.
//!
//! This binary is used in our test suites and experiments (see the Makefile);
//! however, it is not recommended to call it directly.
//!
//! See README for current usage information.

use cargo_scan::download_crate;
use cargo_scan::effect::EffectInstance;
use cargo_scan::scan_stats::{self, CrateStats};
use cargo_scan::util;

use clap::Parser;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use threadpool::ThreadPool;

/*
    Constants
*/

// Number of progress tracking messages to display
const PROGRESS_INCS: usize = 10;

// Source lists
const CRATES_DIR: &str = "data/packages";
const TEST_CRATES_DIR: &str = "data/test-packages";

// Results
const RESULTS_DIR: &str = "data/results";
const RESULTS_ALL_SUFFIX: &str = "_all.csv";
const RESULTS_SUMMARY_SUFFIX: &str = "_summary.csv";
const RESULTS_PATTERNS_SUFFIX: &str = "_patterns.csv";
const RESULTS_METADATA_SUFFIX: &str = "_metadata.csv";

// Whether to remove and re-download old downloaded packages
const UPDATE_DOWNLOADS: bool = true;

/*
    CLI
*/

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Path to a csv file to iterate through
    crates_csv: PathBuf,

    /// Output prefix to save output
    output_prefix: String,

    /// Test run
    #[clap(short, long, default_value_t = false)]
    test_run: bool,

    /// Maximum number of threads to spawn
    #[clap(short, long, default_value_t = 8)]
    num_threads: usize,

    // Run in quick mode (turns off RustAnalyzer)
    #[clap(short, long, default_value_t = false)]
    quick_mode: bool,

    // Don't collect raw list of effects
    #[clap(long, default_value_t = false)]
    skip_raw: bool,

    /// Expand macros and scan expanded code
    #[clap(short, long, default_value_t = false)]
    expand_macro: bool,
}

/*
    Wrapper for scan_stats::get_crate_stats_default
*/
fn crate_stats(
    crt: &str,
    download_loc: PathBuf,
    test_run: bool,
    quick_mode: bool,
    expand_macro: bool,
) -> CrateStats {
    info!("Getting stats for: {}", crt);
    let output_dir = download_loc.join(Path::new(crt));

    if !test_run {
        if UPDATE_DOWNLOADS && output_dir.is_dir() {
            fs::remove_dir_all(&output_dir).expect("failed to remove old dir");
        }

        if !output_dir.is_dir() {
            info!("Downloading {} to: {}", crt, output_dir.to_string_lossy());

            download_crate::download_latest_crate_version(crt, CRATES_DIR)
                .expect("failed to download crate");
        }
    }

    debug!("Downloaded");

    let stats = scan_stats::get_crate_stats_default(output_dir, quick_mode, expand_macro);

    // dbg!(&stats);
    info!("Done scanning: {}", crt);

    stats
}

/*
    Struct to collect stats for all crates
*/
#[derive(Debug, Default)]
struct AllStats {
    crates: Vec<String>,
    crate_stats: HashMap<String, CrateStats>,
    patterns: HashMap<String, usize>,
    crate_patterns: HashMap<String, HashMap<String, usize>>,
}

impl AllStats {
    fn new(crates: Vec<String>) -> Self {
        Self { crates, ..Default::default() }
    }

    fn get_stats(
        self_crate_stats: &mut HashMap<String, CrateStats>,
        crt: String,
    ) -> &mut CrateStats {
        self_crate_stats.entry(crt).or_insert_with(|| {
            warn!("Crate stats not found, possibly due to a crash; using default");
            Default::default()
        })
    }

    fn push_stats(&mut self, crt: String, c: CrateStats) {
        for eff in &c.effects {
            let pat = eff.eff_type().to_csv();
            *self.patterns.entry(pat.clone()).or_default() += 1;
            *self
                .crate_patterns
                .entry(crt.clone())
                .or_default()
                .entry(pat)
                .or_default() += 1;
        }
        if let Some(x) = self.crate_stats.insert(crt, c) {
            warn!("Crate stats already present in map, overwriting: {:?}", x);
        }
    }

    fn dump_all(&mut self, path: &Path) {
        let mut f = util::fs::path_writer(path);
        writeln!(f, "{}", EffectInstance::csv_header()).unwrap();
        for crt in &self.crates {
            let stats = Self::get_stats(&mut self.crate_stats, crt.clone());
            for eff in &stats.effects {
                writeln!(f, "{}", eff.to_csv()).unwrap();
            }
        }
    }

    fn dump_summary(&mut self, path: &Path) {
        let mut f = util::fs::path_writer(path);
        writeln!(f, "crate, effects").unwrap();
        let mut crates_total: Vec<(String, usize)> = self
            .crates
            .iter()
            .map(|k| {
                let stats = Self::get_stats(&mut self.crate_stats, k.to_string());
                (k.to_string(), stats.effects.len())
            })
            .filter(|(_, v)| *v != 0)
            .collect();
        crates_total.sort_by_key(|(_, v)| -(*v as isize));
        for (k, v) in crates_total {
            writeln!(f, "{}, {}", k, v).unwrap();
        }
    }

    fn dump_patterns(&self, path: &Path) {
        let mut f = util::fs::path_writer(path);
        let mut patterns: Vec<String> = self.patterns.keys().cloned().collect();
        patterns.sort();

        write!(f, "crate").unwrap();
        for pat in &patterns {
            write!(f, ", {}", pat).unwrap();
        }
        writeln!(f).unwrap();
        for crt in &self.crates {
            write!(f, "{}", crt).unwrap();
            for pat in &patterns {
                let count = self
                    .crate_patterns
                    .get(crt)
                    .and_then(|x| x.get(pat).cloned())
                    .unwrap_or_default();
                write!(f, ", {}", count).unwrap();
            }
            writeln!(f).unwrap();
        }
    }

    fn dump_metadata(&mut self, path: &Path) {
        let mut f = util::fs::path_writer(path);
        writeln!(f, "crate, {}", CrateStats::metadata_csv_header()).unwrap();
        for crt in &self.crates {
            let stats = Self::get_stats(&mut self.crate_stats, crt.clone());
            writeln!(f, "{}, {}", crt, stats.metadata_csv()).unwrap();
        }
    }
}

/*
    Entrypoint
*/
fn main() {
    cargo_scan::util::init_logging();
    let args = Args::parse();

    println!("===== Scanning all crates in {} =====", args.crates_csv.to_string_lossy());
    debug!("args: {:?}", args);

    let mut rdr = csv::Reader::from_path(&args.crates_csv).expect("Failed to open CSV");
    let _headers = rdr.headers().expect("Failed to read CSV header");

    let mut crates: Vec<String> = Vec::new();
    for row in rdr.records() {
        let record = row.expect("Failed to read CSV row");
        crates.push(record[0].to_string())
    }
    let num_crates = crates.len();

    info!("Scanning {} crates: {:?}", num_crates, crates);

    let download_loc =
        if args.test_run { Path::new(TEST_CRATES_DIR) } else { Path::new(CRATES_DIR) };
    if !download_loc.exists() {
        fs::create_dir_all(download_loc).expect("Failed to create download location");
    }

    let mut all_stats = AllStats::new(crates.clone());

    let batch_size = args.num_threads * 5;
    let num_batches = num_crates.div_ceil(batch_size);
    let progress_inc = num_crates.div_ceil(PROGRESS_INCS);

    for batch in 0..num_batches {
        let pool = ThreadPool::new(args.num_threads);
        let (tx, rx) = mpsc::channel();

        let start = batch * batch_size;
        let end = (batch + 1) * batch_size;
        let end = if end > num_crates { num_crates } else { end };
        let batch_crates = &crates[start..end];

        // Spawn threads
        for crt in batch_crates {
            info!("Spawning thread for: {}", crt);

            let tx_inner = tx.clone();
            let crt = crt.clone();
            let download_loc = download_loc.to_owned();
            pool.execute(move || {
                let res = crate_stats(
                    &crt,
                    download_loc,
                    args.test_run,
                    args.quick_mode,
                    args.expand_macro,
                );
                if let Err(e) = tx_inner.send((crt, res)) {
                    error!("Error sending result: {:?}", e);
                }
            });
        }

        // Drop handle
        drop(tx);
        // Wait for threads
        info!("Waiting for threads... (batch {} of {})", batch, num_batches);
        for (i, (crt, stats)) in rx.iter().enumerate() {
            all_stats.push_stats(crt, stats);
            if (start + i + 1) % progress_inc == 0 {
                println!(
                    "{:.0}% complete",
                    ((100 * (start + i + 1)) as f64) / (num_crates as f64)
                );
            }
        }
    }

    // dbg!(&all_stats);

    // Save Results
    let base = Path::new(RESULTS_DIR);
    let pref = args.output_prefix;
    let output_all = base.join(pref.to_string() + RESULTS_ALL_SUFFIX);
    let output_summary = base.join(pref.to_string() + RESULTS_SUMMARY_SUFFIX);
    let output_pattern = base.join(pref.to_string() + RESULTS_PATTERNS_SUFFIX);
    let output_metadata = base.join(pref.to_string() + RESULTS_METADATA_SUFFIX);

    println!("Saving summary, patterns, and metadata to: {}", base.to_string_lossy());
    all_stats.dump_summary(&output_summary);
    all_stats.dump_patterns(&output_pattern);
    all_stats.dump_metadata(&output_metadata);

    if !args.skip_raw {
        println!("Saving raw effect list to: {}", output_all.to_string_lossy());
        all_stats.dump_all(&output_all);
    }
}
