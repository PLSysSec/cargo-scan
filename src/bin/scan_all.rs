//! Run a scan for a list of crates in parallel.

use cargo_scan::effect::EffectInstance;
use cargo_scan::scan_stats::{self, CrateStats};
use cargo_scan::util;

use clap::Parser;
use log::{error, info};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::channel;
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
const RESULTS_PATTERN_SUFFIX: &str = "_pattern.txt";
const RESULTS_SUMMARY_SUFFIX: &str = "_summary.txt";
const RESULTS_METADATA_SUFFIX: &str = "_metadata.csv";

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
}

fn crate_stats(crt: &str, download_loc: PathBuf, test_run: bool) -> CrateStats {
    info!("Getting stats for: {}", crt);
    let output_dir = download_loc.join(Path::new(crt));

    if !test_run {
        let _output = Command::new("cargo")
            .arg("download")
            .arg("-x")
            .arg(crt)
            .arg("-o")
            .arg(&output_dir)
            .output()
            .expect("failed to run cargo download");
    }

    let stats = scan_stats::get_crate_stats_default(output_dir)
        .expect("Failed to get crate stats");

    // TODO: disabled for running locally; consider uncommenting again
    // remove_dir_all(output_dir)?;

    // dbg!(&stats);
    info!("Done scanning: {}", crt);

    stats
}

#[derive(Debug, Default)]
struct AllStats {
    crates: Vec<String>,
    crate_stats: HashMap<String, CrateStats>,
}

impl AllStats {
    fn new(crates: Vec<String>) -> Self {
        Self { crates, ..Default::default() }
    }
    fn push_stats(&mut self, crt: String, c: CrateStats) {
        self.crate_stats.insert(crt, c);
    }

    fn dump_all(&self, path: &Path) {
        let mut f = util::fs::path_writer(path);
        writeln!(f, "{}", EffectInstance::csv_header()).unwrap();
        for crt in &self.crates {
            let stats = self.crate_stats.get(crt).unwrap();
            for eff in &stats.effects {
                writeln!(f, "{}", eff.to_csv()).unwrap();
            }
        }
    }
    fn dump_pattern(&self, _path: &Path) {
        // TODO
    }
    fn dump_summary(&self, _path: &Path) {
        // TODO
    }
    fn dump_metadata(&self, path: &Path) {
        let mut f = util::fs::path_writer(path);
        writeln!(f, "crate, {}", CrateStats::metadata_csv_header()).unwrap();
        for crt in &self.crates {
            let stats = self.crate_stats.get(crt).unwrap();
            writeln!(f, "{}, {}", crt, stats.metadata_csv()).unwrap();
        }
    }
}

fn main() {
    cargo_scan::util::init_logging();
    let args = Args::parse();

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

    let pool = ThreadPool::new(args.num_threads);
    let (tx, rx) = channel();

    for crt in &crates {
        let tx = tx.clone();
        let crt = crt.clone();
        let download_loc = download_loc.to_owned();
        let test_run = args.test_run;
        pool.execute(move || {
            let res = crate_stats(&crt, download_loc, test_run);
            if let Err(e) = tx.send((crt, res)) {
                error!("Error sending result: {:?}", e);
            }
        });

        // TODO: is this line necessary?
        assert!(rx.try_iter().next().is_none());
        // Old:
        // Clean up our waiting threads
        // for (crt, stats) in rx.try_iter() {
        //     stats.push(msg);
        // }
    }

    // Drop our last channel so we don't block forever waiting for it to finish
    drop(tx);

    info!("Waiting for jobs to complete...");

    // Collect the messages
    let mut all_stats = AllStats::new(crates);
    let progress_inc = num_crates / PROGRESS_INCS;
    for (i, (crt, stats)) in rx.iter().enumerate() {
        all_stats.push_stats(crt, stats);
        if (i + 1) % progress_inc == 0 {
            println!("{:.0}% complete", ((100 * (i + 1)) as f64) / (num_crates as f64));
        }
    }

    // dbg!(&stats);

    // Save Results
    let base = Path::new(RESULTS_DIR);
    let pref = args.output_prefix;
    let output_all = base.join(pref.to_string() + RESULTS_ALL_SUFFIX);
    let output_pattern = base.join(pref.to_string() + RESULTS_PATTERN_SUFFIX);
    let output_summary = base.join(pref.to_string() + RESULTS_SUMMARY_SUFFIX);
    let output_metadata = base.join(pref.to_string() + RESULTS_METADATA_SUFFIX);

    all_stats.dump_all(&output_all);
    all_stats.dump_pattern(&output_pattern);
    all_stats.dump_summary(&output_summary);
    all_stats.dump_metadata(&output_metadata);
}
