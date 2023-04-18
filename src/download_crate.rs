use std::fs::{File, remove_file};
use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;
use cargo_lock::Package;
use curl::easy::Easy;
use flate2::read::GzDecoder;
use tar::Archive;

pub fn download_crate(package: &Package, download_dir: &str) -> Result<PathBuf> {
    let url = match &package.source {
        Some(source) => source.url().as_str().to_string(),
        None => {
            format!(
                "https://crates.io/api/v1/crates/{}/{}/download",
                package.name.as_str(),
                package.version
            )
        }
    };
    let mut dst = Vec::new();
    let mut easy = Easy::new();
    easy.url(&url)?;

    {
        let mut transfer = easy.transfer();
        transfer.write_function(|data| {
            dst.extend_from_slice(data);
            Ok(data.len())
        })?;
        transfer.perform()?;
    }

    let tarball_name = format!("{}-{}.tar.gz", package.name.as_str(), package.version);
    let mut download_dir = PathBuf::from(download_dir);
    download_dir.push(tarball_name.clone());

    {
        let mut tarball_file = File::create(download_dir.as_os_str())?;
        tarball_file.write_all(&dst)?;
        let tar = GzDecoder::new(tarball_file);
        let mut archive = Archive::new(tar);
        archive.unpack(download_dir.clone())?;
    }

    remove_file(tarball_name)?;

    download_dir.pop();
    download_dir.push(format!("{}-{}", package.name.as_str(), package.version));

    Ok(download_dir)
}
