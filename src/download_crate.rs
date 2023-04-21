use std::fs::{remove_file, File};
use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;
use cargo_lock::Package;
use curl::easy::Easy;
use flate2::read::GzDecoder;
use tar::Archive;

fn get_crates_io_url(package: &Package) -> String {
    format!(
        "https://crates.io/api/v1/crates/{}/{}/download",
        package.name.as_str(),
        package.version
    )
}

pub fn download_crate(package: &Package, download_dir: &str) -> Result<PathBuf> {
    let url = match &package.source {
        // TODO: This is a bit of a hack to handle crates.io urls. We should
        //       handle non crates.io urls as well.
        Some(source) => {
            let source_str = source.url().as_str().to_string();
            if source_str == "https://github.com/rust-lang/crates.io-index" {
                get_crates_io_url(package)
            } else {
                source_str
            }
        }
        None => get_crates_io_url(package),
    };

    dbg!(&url);
    let mut dst = Vec::new();
    let mut easy = Easy::new();
    easy.follow_location(true)?;
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
    dbg!(&download_dir);

    {
        let tarball_file = File::open(download_dir.clone())?;
        let tar = GzDecoder::new(tarball_file);
        let mut archive = Archive::new(tar);
        download_dir.pop();
        archive.unpack(download_dir.clone())?;
    }

    remove_file(tarball_name)?;

    download_dir.pop();
    download_dir.push(format!("{}-{}", package.name.as_str(), package.version));

    Ok(download_dir)
}
