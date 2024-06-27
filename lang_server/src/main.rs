pub mod server;

use env_logger::Builder;
use log::info;
use std::{env, error::Error, str::FromStr};

fn main() -> anyhow::Result<(), Box<dyn Error + Sync + Send>> {
    // Initialize logger
    let log_var = env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let level = log::LevelFilter::from_str(&log_var)?;

    Builder::from_env(log_var)
        .filter_module("lang_server", level)
        .filter_module("cargo_scan", level)
        .init();

    info!("Starting cargo-scan LSP server");
    server::run_server()?;
    info!("Shutting down cargo-scan server!");

    Ok(())
}
