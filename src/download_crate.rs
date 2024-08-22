use std::fs::{create_dir_all, remove_file, write, File};
use std::path::PathBuf;
use std::process::Command;

use anyhow::{anyhow, Result};
use cargo_lock::Package;
use curl::easy::Easy;
use flate2::read::GzDecoder;
use regex::Regex;
use log::info;
use tar::Archive;

fn get_crates_io_url(package_name: &str, package_version: &str) -> String {
    format!(
        "https://crates.io/api/v1/crates/{}/{}/download",
        package_name, package_version
    )
}

fn download_crate(
    url: &str,
    package_name: &str,
    package_version: &str,
    download_dir: &str,
) -> Result<PathBuf> {
    let mut dst = Vec::new();
    let mut easy = Easy::new();
    easy.follow_location(true)?;
    easy.url(url)?;

    {
        let mut transfer = easy.transfer();
        transfer.write_function(|data| {
            dst.extend_from_slice(data);
            Ok(data.len())
        })?;
        transfer.perform()?;
    }

    let package_dir_name = format!("{}-{}", package_name, package_version);
    let tarball_name = format!("{}.tar.gz", package_dir_name);
    let mut download_dir = PathBuf::from(download_dir);
    download_dir.push(tarball_name.clone());
    write(&download_dir, dst)?;

    {
        let tarball_file = File::open(download_dir.clone())?;
        let tar = GzDecoder::new(tarball_file);
        let mut archive = Archive::new(tar);

        // Set the download_dir to the expected crate download dir
        download_dir.pop();
        download_dir.push(&package_dir_name);

        // if the directory already exists, delete it and use the new version;
        // we redownload to make sure that e.g. non-crates.io versions with the
        // same semver are still downloaded
        if download_dir.exists() {
            info!(
                "Another instance of this crate already exists, downloading new version"
            );
            std::fs::remove_dir_all(download_dir.clone())?;
        }

        // pop the last folder because the extraction will include a new folder
        // for the package
        download_dir.pop();
        create_dir_all(download_dir.clone())?;
        archive.unpack(download_dir.clone())?;
    }

    download_dir.push(tarball_name);
    remove_file(download_dir.clone())?;

    download_dir.pop();
    download_dir.push(package_dir_name);

    Ok(download_dir)
}

/// Downloads the crate from the package name and version
pub fn download_crate_from_info(
    package_name: &str,
    package_version: &str,
    download_dir: &str,
) -> Result<PathBuf> {
    let url = get_crates_io_url(package_name, package_version);
    download_crate(&url, package_name, package_version, download_dir)
}

/// Get the latest version of a crate from only the package name.
pub fn get_latest_version(
    package_name: &str,
) -> Result<String> {

    // Query `cargo search`.
    let result = Command::new("cargo")
        .arg("search")
        .arg(package_name)
        .arg("--limit")
        .arg("1")
        .output()?;

    // Convert the output to a string.
    let output = String::from_utf8(result.stdout)?;

    // Debug
    println!("{:?}", output);

    // Parse the output. It should contain <crate name> = "<version>"
    let re = Regex::new(r#"^([a-zA-Z0-9_-]+) = "(\d+\.\d+\.\d+)""#).unwrap();
    if let Some(caps) = re.captures(&output) {
        let name = &caps[1];
        let version = &caps[2];
        if name == package_name {
            // Debug
            println!("Found version: {} for package: {}", version, package_name);

            return Ok(version.to_string());
        }
    }

    Err(anyhow!("No match found for package name: {}", package_name))
}

/// Downloads the latest version of a crate from only the package name
pub fn download_latest_crate_version(
    package_name: &str,
    download_dir: &str,
) -> Result<PathBuf> {
    let latest_version = get_latest_version(package_name)?;
    download_crate_from_info(package_name, &latest_version, download_dir)
}

/// Downloads the crate from the `cargo_lock::Package`
pub fn download_crate_from_package(
    package: &Package,
    download_dir: &str,
) -> Result<PathBuf> {
    let url = match &package.source {
        // TODO: This is a bit of a hack to handle crates.io urls. We should
        //       handle non crates.io urls as well.
        Some(source) => {
            let source_str = source.url().as_str().to_string();
            if source_str == "https://github.com/rust-lang/crates.io-index" {
                get_crates_io_url(package.name.as_str(), &package.version.to_string())
            } else {
                source_str
            }
        }
        None => get_crates_io_url(package.name.as_str(), &package.version.to_string()),
    };

    download_crate(
        &url,
        package.name.as_ref(),
        &package.version.to_string(),
        download_dir,
    )
}
