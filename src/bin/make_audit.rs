use cargo_scan::audit_chain::AuditChain;
use cargo_scan::download_crate;
use cargo_scan::policy::PolicyFile;

use anyhow::{anyhow, Context, Result};
use cargo_lock::{Lockfile, Package};
use clap::{Args as ClapArgs, Parser, Subcommand};
use std::fs::{create_dir_all, read_to_string};
use std::path::PathBuf;
use toml::{self, value::Table};

#[derive(Parser, Debug)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Create(Create),
    Audit(Audit),
}

// TODO: Add an argument for the default policy type
#[derive(Clone, ClapArgs, Debug)]
struct Create {
    /// Path to manifest
    manifest_path: String,
    /// Path to crate
    crate_path: String,

    // TODO: Can probably use the default rust build location
    /// Path to download crates to for auditing
    #[clap(short = 'd', long = "crate-download-path", default_value = ".audit_crates")]
    crate_download_path: String,

    // TODO: Check to make sure it meets the format (clap supports this?)
    /// Default policy folder
    #[clap(short = 'p', long = "policy-path", default_value = ".audit_policies")]
    policy_path: String,
}

#[derive(Clone, ClapArgs, Debug)]
struct Audit {
    /// Path to manifest
    manifest_path: String,
}

// TODO: Different default policies
/// Creates a new default policy for the given package and returns the path to
/// the saved policy file
fn make_new_policy(package: &Package, root_name: &str, args: &Create) -> Result<PathBuf> {
    let policy_path = PathBuf::from(format!(
        "{}/{}-{}.policy",
        args.policy_path,
        package.name.as_str(),
        package.version
    ));

    // download the new policy
    let package_path = if package.name.as_str() == root_name {
        // We are creating a policy for the root crate
        PathBuf::from(args.crate_path.clone())
    } else {
        // TODO: Handle the case where we have a crate source not from crates.io
        download_crate::download_crate(package, &args.crate_download_path)?
    };

    // Try to create a new default policy
    if policy_path.is_dir() {
        return Err(anyhow!("Policy path is a directory"));
    }
    if policy_path.is_file() {
        return Err(anyhow!("Policy file already exists"));
    }

    let policy = PolicyFile::new_caller_checked_default(package_path.as_path())?;
    policy.save_to_file(policy_path.clone())?;

    Ok(policy_path)
}

fn create_audit_chain_dirs(args: &Create) -> Result<()> {
    let mut manifest_path = PathBuf::from(&args.manifest_path);
    manifest_path.pop();
    create_dir_all(manifest_path)?;

    let crate_download_path = PathBuf::from(&args.crate_download_path);
    create_dir_all(crate_download_path)?;

    let policy_path = PathBuf::from(&args.policy_path);
    create_dir_all(policy_path)?;

    Ok(())
}

fn create_new_audit_chain(args: Create) -> Result<AuditChain> {
    let mut chain = AuditChain::new(
        PathBuf::from(&args.manifest_path),
        PathBuf::from(&args.crate_path),
    );

    create_audit_chain_dirs(&args)?;

    let lockfile = Lockfile::load(format!("{}/Cargo.lock", args.crate_path))?;

    let toml_string =
        read_to_string(PathBuf::from(format!("{}/Cargo.toml", args.crate_path)))?;
    let cargo_toml = toml::from_str::<Table>(&toml_string).context("Couldn't parse Cargo.toml")?;
    let root_name = cargo_toml
        .get("package")
        .context("No package in Cargo.toml")?
        .as_table()
        .context("Package field is not a table")?
        .get("name")
        .context("No name for the package in Cargo.toml")?
        .as_str()
        .context("Name field in package is not a string")?;

    for package in lockfile.packages {
        match make_new_policy(&package, root_name, &args) {
            Ok(policy_path) => {
                chain.add_crate_policy(&package, policy_path);
            }
            Err(e) => return Err(anyhow!("Audit chain creation failed: {}", e)),
        };
    }

    Ok(chain)
}

fn runner(args: Args) -> Result<()> {
    match args.command {
        Command::Create(create) => {
            let chain = create_new_audit_chain(create)?;
            chain.save_to_file()?;
            Ok(())
        }
        Command::Audit(_audit) => Ok(()),
    }
}

fn main() {
    let args = Args::parse();

    match runner(args) {
        Ok(()) => (),
        Err(e) => println!("Error running command: {}", e),
    }
}
