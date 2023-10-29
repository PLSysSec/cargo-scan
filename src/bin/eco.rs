//! Run a scan for a list of crates in parallel.

use cargo_scan::scan_stats::{self, CrateStats};

use anyhow::Result;
use clap::Parser;
use serde::Deserialize;
use std::fs::create_dir_all;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::channel;
use threadpool::ThreadPool;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Path to a csv file to iterate through
    crates_csv: PathBuf,

    /// Temp directory for crate downloads
    #[clap(short, long, default_value = "./.eco_crates_tmp")]
    download_loc: PathBuf,

    /// Maximum number of threads to spawn
    #[clap(short, long, default_value_t = 4)]
    num_threads: usize,
}

#[derive(Debug, Deserialize)]
struct Record {
    name: String,
    _downloads: usize,
    _description: String,
    _created_at: String,
    _updated_at: String,
    _documentation: String,
    _homepage: String,
    _repository: String,
    _id: usize,
}

fn crate_stats(record: Record, download_dir: String) -> Result<CrateStats> {
    println!("Getting stats for: {}", &record.name);
    let output_dir = format!("{}/{}", download_dir, &record.name);
    let _output = Command::new("cargo")
        .arg("download")
        .arg("-x")
        .arg(&record.name)
        .args(["-o", &output_dir])
        .output()?;

    let stats = scan_stats::get_crate_stats_default(PathBuf::from(record.name))?;

    // TODO: disabled for running locally; consider uncommenting again
    // remove_dir_all(output_dir)?;

    Ok(stats)
}

fn main() -> Result<()> {
    let args = Args::parse();

    // let csv_contents = read_to_string(args.crates_csv)?;

    let mut rdr = csv::Reader::from_path(&args.crates_csv)?;
    let _headers = rdr.headers()?;
    let records = rdr.deserialize::<Record>().flatten();
    let mut stats = Vec::new();

    // Make sure the download location exists
    if !args.download_loc.exists() {
        create_dir_all(&args.download_loc)?;
    }

    let pool = ThreadPool::new(args.num_threads);
    let (tx, rx) = channel();
    let download_loc_path = args.download_loc.as_path();
    let download_loc = download_loc_path.to_string_lossy();

    for r in records {
        let tx = tx.clone();
        let d: String = download_loc.to_string();
        pool.execute(move || {
            if let Ok(res) = crate_stats(r, d) {
                if let Err(e) = tx.send(res) {
                    println!("Error sending result: {:?}", e);
                }
            }
        });

        // Clean up our waiting threads
        for msg in rx.try_iter() {
            stats.push(msg);
        }
    }

    // Drop our last channel so we don't block forever waiting for it to finish
    drop(tx);

    // Get the last waiting messages
    for msg in rx.iter() {
        stats.push(msg);
    }

    // TODO: Do your thing with stats
    dbg!(&stats);

    Ok(())
}
