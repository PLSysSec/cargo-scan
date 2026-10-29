//! Utility functions

/// Initialize logging for all of cargo_scan
///
/// To change the log level, run with e.g.:
/// RUST_LOG=debug cargo run --bin scan ...
/// RUST_LOG=info cargo run --bin scan ...
pub fn init_logging() {
    use env_logger::Builder;
    use std::env;

    // wish there was a nicer way to do this, env_logger doesn't make it easy
    // to disable non-cargo_scan logging
    let level = env::var("RUST_LOG").as_deref().unwrap_or("warn").to_string();
    let filters = format!(
        "warn,cargo_scan={},scan={},audit={},chain={}",
        level, level, level, level
    );

    Builder::new().parse_filters(&filters).init();
}

/// CSV utility functions
pub mod csv {
    use log::warn;
    use std::path::Path;

    pub fn sanitize(s: &str) -> String {
        s.replace(',', "\\,")
    }
    pub fn sanitize_path(p: &Path) -> String {
        match p.to_str() {
            Some(s) => sanitize(s),
            None => {
                warn!("Warning: path is invalid unicode: {:?}", p);
                sanitize(&p.to_string_lossy())
            }
        }
    }
}

/// Iterator util
pub mod iter {
    use log::warn;
    use std::fmt::Display;
    use std::vec;

    /// Ignore errors, printing them to stderr
    /// useful with iter::filter_map: `my_iter.filter_map(warn_ok)`
    pub fn warn_ok<T, E: Display>(x: Result<T, E>) -> Option<T> {
        if let Some(e) = x.as_ref().err() {
            warn!("Warning: discarding error {}", e);
        }
        x.ok()
    }

    /// Convert an iterator into one that owns all its elements
    pub trait FreshIter {
        type Result: Iterator;
        fn fresh_iter(self) -> Self::Result;
    }
    impl<I: Iterator> FreshIter for I {
        type Result = vec::IntoIter<I::Item>;
        fn fresh_iter(self) -> Self::Result {
            self.collect::<Vec<I::Item>>().into_iter()
        }
    }
}

/// Filesystem util
pub mod fs {
    use std::fmt::Debug;
    use std::fs::File;
    use std::io::{BufRead, BufReader, BufWriter};
    use std::path::{Path, PathBuf};
    use walkdir::{DirEntry, WalkDir};

    pub fn walk_files(p: &Path) -> impl Iterator<Item = PathBuf> {
        debug_assert!(p.is_dir());
        WalkDir::new(p)
            .sort_by_file_name()
            .into_iter()
            .filter_map(super::iter::warn_ok)
            .map(DirEntry::into_path)
    }

    pub fn walk_files_with_extension<'a>(
        p: &'a Path,
        ext: &'a str,
    ) -> impl Iterator<Item = PathBuf> + 'a {
        walk_files(p)
            .filter(|entry| entry.is_file())
            .filter(|entry| entry.extension().is_some_and(|x| x.to_str() == Some(ext)))
    }

    pub fn file_lines(p: &PathBuf) -> impl Iterator<Item = String> {
        let file = File::open(p).unwrap();
        let reader = BufReader::new(file).lines();
        reader.map(|line| line.unwrap())
    }

    pub fn path_writer<P>(path: P) -> BufWriter<File>
    where
        P: AsRef<Path> + Debug,
    {
        BufWriter::new(File::create(&path).unwrap_or_else(|err| {
            panic!("Could not open file for writing: {:?} -- {}", path, err)
        }))
    }
}

/// Parse Cargo TOML
use anyhow::Result;
use cargo_lock::Package;
use log::debug;
use semver::Version;
use serde::de::{self, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::path::Path;

#[derive(Eq, Hash, PartialEq, Debug, Clone)]
pub struct CrateId {
    pub crate_name: String,
    pub version: Version,
}

impl Serialize for CrateId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}:{}", self.crate_name, self.version))
    }
}

struct CrateIdVisitor;

impl Visitor<'_> for CrateIdVisitor {
    type Value = CrateId;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a colon-separated pair of the crate name and version")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let mut split = s.split(':');
        match (split.next(), split.next(), split.next()) {
            (Some(crate_name), Some(crate_version), None) => {
                if let Ok(version) = Version::parse(crate_version) {
                    Ok(CrateId { crate_name: crate_name.to_string(), version })
                } else {
                    Err(de::Error::invalid_value(Unexpected::Str(s), &self))
                }
            }
            _ => Err(de::Error::invalid_value(Unexpected::Str(s), &self)),
        }
    }
}

impl<'a> Deserialize<'a> for CrateId {
    fn deserialize<D>(deserializer: D) -> Result<CrateId, D::Error>
    where
        D: Deserializer<'a>,
    {
        deserializer.deserialize_string(CrateIdVisitor)
    }
}

impl From<&Package> for CrateId {
    fn from(package: &Package) -> Self {
        CrateId { crate_name: package.name.to_string(), version: package.version.clone() }
    }
}

impl From<&cargo::workspace::Package> for CrateId {
    fn from(package: &cargo::workspace::Package) -> Self {
        let pkg_version = package.version();
        let version =
            Version::new(pkg_version.major, pkg_version.minor, pkg_version.patch);
        CrateId { crate_name: package.name().to_string(), version }
    }
}

impl From<&cargo::workspace::PackageId> for CrateId {
    fn from(pkg_id: &cargo::workspace::PackageId) -> Self {
        let pkg_version = pkg_id.version();
        let version =
            Version::new(pkg_version.major, pkg_version.minor, pkg_version.patch);
        CrateId { crate_name: pkg_id.name().to_string(), version }
    }
}

impl CrateId {
    pub fn new(name: String, version: Version) -> Self {
        CrateId { crate_name: name, version }
    }
}

impl fmt::Display for CrateId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}-{}", self.crate_name, self.version)
    }
}

/// Load the name and version of the package in `crate_path`.
///
/// See [`crate::cargo_workspace::load_package`]; errors on virtual workspaces.
pub fn load_cargo_toml(crate_path: &Path) -> Result<CrateId> {
    debug!("Loading Cargo.toml at: {:?}", crate_path);
    let result = crate::cargo_workspace::load_package(crate_path)?.id;
    debug!("Loaded: {:?}", result);
    Ok(result)
}
