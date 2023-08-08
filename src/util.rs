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
    let filters = "warn,cargo_scan=".to_string()
        + env::var("RUST_LOG").as_deref().unwrap_or("warn");

    Builder::new().parse_filters(&filters).init();
}

/// CSV utility functions
pub mod csv {
    use log::warn;
    use std::path::Path;

    pub fn sanitize_comma(s: &str) -> String {
        if s.contains(',') {
            warn!("Warning: ignoring unexpected comma when generating CSV: {s}");
        }
        s.replace(',', "")
    }
    pub fn sanitize_path(p: &Path) -> String {
        match p.to_str() {
            Some(s) => sanitize_comma(s),
            None => {
                warn!("Warning: path is invalid unicode: {:?}", p);
                sanitize_comma(&p.to_string_lossy())
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
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use std::path::PathBuf;
    use walkdir::{DirEntry, WalkDir};

    pub fn walk_files(p: &PathBuf) -> impl Iterator<Item = PathBuf> {
        debug_assert!(p.is_dir());
        WalkDir::new(p)
            .sort_by_file_name()
            .into_iter()
            .filter_map(super::iter::warn_ok)
            .map(DirEntry::into_path)
    }

    pub fn walk_files_with_extension<'a>(
        p: &'a PathBuf,
        ext: &'a str,
    ) -> impl Iterator<Item = PathBuf> + 'a {
        walk_files(p)
            .filter(|entry| entry.is_file())
            .filter(|entry| entry.extension().map_or(false, |x| x.to_str() == Some(ext)))
    }

    pub fn file_lines(p: &PathBuf) -> impl Iterator<Item = String> {
        let file = File::open(p).unwrap();
        let reader = BufReader::new(file).lines();
        reader.map(|line| line.unwrap())
    }
}

/// Parse Cargo TOML
use anyhow::{Context, Result};
use cargo_lock::{Dependency, Package};
use log::debug;
use semver::Version;
use serde::de::{self, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::fs::read_to_string;
use std::path::Path;
use toml::{self, value::Table};

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

impl<'de> Visitor<'de> for CrateIdVisitor {
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

impl From<&Dependency> for CrateId {
    fn from(dep: &Dependency) -> Self {
        CrateId { crate_name: dep.name.to_string(), version: dep.version.clone() }
    }
}

impl CrateId {
    pub fn from_toml_package(package: &cargo_toml::Package) -> Result<Self> {
        Ok(CrateId {
            crate_name: package.name.to_string(),
            version: Version::parse(package.version())?,
        })
    }

    pub fn new(name: String, version: Version) -> Self {
        CrateId { crate_name: name, version }
    }
}

impl fmt::Display for CrateId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}-{}", self.crate_name, self.version)
    }
}

#[derive(Debug, Clone)]
pub struct CrateData {
    pub name: String,
    pub version: String,
}

pub fn load_cargo_toml(crate_path: &Path) -> Result<CrateData> {
    debug!("Loading Cargo.toml at: {:?}", crate_path);

    let toml_string = read_to_string(crate_path.join("Cargo.toml"))?;
    let cargo_toml =
        toml::from_str::<Table>(&toml_string).context("Couldn't parse Cargo.toml")?;
    let root_toml_table = cargo_toml
        .get("package")
        .context("No package in Cargo.toml")?
        .as_table()
        .context("Package field is not a table")?;
    let name = root_toml_table
        .get("name")
        .context("No name for the root package in Cargo.toml")?
        .as_str()
        .context("name field in package is not a string")?
        .to_string();

    // TODO: The reality of finding the version is more messy than this because
    //       you have to track things like the current workspace and virtual
    //       workspaces. For now, we will just assume things are nice.
    let version = match root_toml_table
        .get("version")
        .context("No version for the root package in Cargo.toml")?
        .as_str()
    {
        Some(v) => v.to_string(),
        None => {
            // Look up the version from the workspace.package section
            let workspace_table = cargo_toml
                .get("workspace")
                .context("Missing version in package section and no workspace section")?
                .as_table()
                .context("workpace is not a table")?
                .get("package")
                .context("Missing version in package section and no workspace.package")?
                .as_table()
                .context("workspace.package is not a table")?;

            workspace_table
                .get("version")
                .context(
                    "No version entry in package section or workspace.package section",
                )?
                .as_str()
                .context("version entry in workspace.package is not a string")?
                .to_string()
        }
    };

    let result = CrateData { name, version };
    debug!("Loaded: {:?}", result);
    Ok(result)
}
