use std::fs::{create_dir_all, remove_dir_all};
use std::path::PathBuf;
use std::process::Command;

use anyhow::Result;
use cargo_scan::audit_file::AuditFile;
use cargo_scan::effect::DEFAULT_EFFECT_TYPES;
// use cargo_scan::scanner::ScanResults;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::channel;
use threadpool::ThreadPool;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Path to a csv file to iterate through
    csv_file: PathBuf,

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
    downloads: usize,
    description: String,
    created_at: String,
    updated_at: String,
    documentation: String,
    homepage: String,
    repository: String,
    id: usize,
}

// TODO: populate with stats from the python script
#[derive(Debug, Serialize)]
struct CrateStats {
    crate_name: String,
    num_pub_funs: usize,
    num_pub_funs_caller_checked: usize,
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

    let (audit, results) = AuditFile::new_caller_checked_default_with_results(
        &PathBuf::from(&output_dir),
        &DEFAULT_EFFECT_TYPES,
    )?;

    // TODO: Populate more crate stats
    let stats = CrateStats {
        crate_name: record.name,
        num_pub_funs: results.pub_fns.len(),
        num_pub_funs_caller_checked: audit.pub_caller_checked.len(),
    };

    remove_dir_all(output_dir)?;

    Ok(stats)
}

fn main() -> Result<()> {
    let args = Args::parse();

    // let csv_contents = read_to_string(args.csv_file)?;

    let mut rdr = csv::Reader::from_path(&args.csv_file)?;
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
        let mut iter = rx.try_iter();
        while let Some(msg) = iter.next() {
            stats.push(msg);
        }
    }

    // Drop our last channel so we don't block forever waiting for it to finish
    drop(tx);

    // Get the last waiting messages
    let mut iter = rx.iter();
    while let Some(msg) = iter.next() {
        stats.push(msg);
    }

    // TODO: Do your thing with stats
    dbg!(&stats);

    Ok(())
}
