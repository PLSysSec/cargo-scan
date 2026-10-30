//! Cargo workspace model shared by the scanner and audit chains.
//!
//! Packages and workspaces are loaded with Cargo itself, so workspace
//! inheritance, member globs, `exclude`, and target names follow Cargo's rules
//! exactly.

use crate::util::CrateId;

use anyhow::{anyhow, Result};
use cargo::util::GlobalContext;
use cargo::workspace::{Package, TargetKind, Workspace};
use itertools::Itertools;
use log::debug;
use std::path::{Path, PathBuf};

/// A build target of a package (lib, bin, ...)
#[derive(Debug, Clone)]
pub struct TargetInfo {
    /// Name used for the target's crate in paths,
    /// e.g. `my_crate` for package `my-crate`
    pub crate_name: String,
    pub kind: TargetKind,
    /// Root source file of the target, if it has one
    pub root: Option<PathBuf>,
}

/// A package as loaded by Cargo
#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub id: CrateId,
    /// Directory containing the package's Cargo.toml
    pub dir: PathBuf,
    pub targets: Vec<TargetInfo>,
}

impl From<&Package> for PackageInfo {
    fn from(pkg: &Package) -> Self {
        let targets = pkg
            .targets()
            .iter()
            .map(|t| TargetInfo {
                crate_name: t.crate_name(),
                kind: t.kind().clone(),
                root: t.src_path().path().map(Path::to_path_buf),
            })
            .collect();
        Self {
            id: CrateId::new(pkg.name().to_string(), pkg.version().clone()),
            dir: pkg.root().to_path_buf(),
            targets,
        }
    }
}

/// Make `path` absolute and remove `.` and `..` components, without resolving
/// symlinks.
///
/// This is the normalization Cargo uses, so paths normalized here match the
/// paths Cargo and rust-analyzer report for the same input. We need this
/// instead of [`std::fs::canonicalize`] to keep paths consistent and compare
/// them correctly.
pub fn normalize_path(path: &Path) -> Result<PathBuf> {
    Ok(cargo_util::paths::normalize_path(&std::path::absolute(path)?))
}

/// Get the normalized path to the Cargo.toml of the package or workspace in
/// `crate_path`.
pub fn manifest_path(crate_path: &Path) -> Result<PathBuf> {
    if !crate_path.is_dir() {
        return Err(anyhow!("Path is not a crate; not a directory: {:?}", crate_path));
    }
    let manifest = normalize_path(&crate_path.join("Cargo.toml"))?;
    if !manifest.is_file() {
        return Err(anyhow!("Path is not a crate; missing Cargo.toml: {:?}", crate_path));
    }
    Ok(manifest)
}

/// Load the Cargo workspace containing the package or workspace in `crate_path`
pub fn load_workspace<'gctx>(
    crate_path: &Path,
    gctx: &'gctx GlobalContext,
) -> Result<Workspace<'gctx>> {
    let manifest = manifest_path(crate_path)?;
    debug!("Loading Cargo workspace for: {:?}", manifest);
    Workspace::new(&manifest, gctx)
}

/// Load the single package in `crate_path`.
///
/// Errors if `crate_path` is a virtual workspace; use [`load_members`] for
/// those.
pub fn load_package(crate_path: &Path) -> Result<PackageInfo> {
    let gctx = GlobalContext::default()?;
    let ws = load_workspace(crate_path, &gctx)?;
    if ws.is_virtual() {
        let members = ws.members().map(|p| p.name()).join(", ");
        return Err(anyhow!(
            "{:?} is a virtual workspace, not a package (members: {})",
            crate_path,
            members
        ));
    }
    Ok(ws.current()?.into())
}

/// Load all member packages of the workspace in `crate_path`.
///
/// For a package that is not part of a larger workspace, this is just the
/// package itself.
pub fn load_members(crate_path: &Path) -> Result<Vec<PackageInfo>> {
    let gctx = GlobalContext::default()?;
    let ws = load_workspace(crate_path, &gctx)?;
    Ok(ws.members().map(PackageInfo::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::Version;
    use std::fs;
    use tempfile::TempDir;

    /// Write `contents` to `<root>/<dir>/Cargo.toml` and add an empty
    /// `src/lib.rs`, returning the package directory.
    fn write_manifest(root: &TempDir, dir: &str, contents: &str) -> PathBuf {
        let pkg = root.path().join(dir);
        fs::create_dir_all(pkg.join("src")).unwrap();
        fs::write(pkg.join("Cargo.toml"), contents).unwrap();
        fs::write(pkg.join("src/lib.rs"), "").unwrap();
        pkg
    }

    fn package(name: &str, version: &str) -> String {
        format!("[package]\nname = \"{name}\"\nversion = {version}\nedition = \"2021\"\n")
    }

    #[test]
    fn test_normalize_path() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(normalize_path(Path::new("a/./b/../c")).unwrap(), cwd.join("a/c"));
        assert_eq!(
            normalize_path(Path::new("../d")).unwrap(),
            cwd.parent().unwrap().join("d")
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_normalize_path_keeps_symlinks() {
        let root = TempDir::new().unwrap();
        let pkg = write_manifest(&root, "real", &package("a", "\"0.1.0\""));
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&pkg, &link).unwrap();

        assert_eq!(normalize_path(&link.join("src/../src")).unwrap(), link.join("src"));
        assert_eq!(manifest_path(&link).unwrap(), link.join("Cargo.toml"));
        // Cargo keeps the symlinked path too, which is what `normalize_path`
        // relies on to match it
        assert_eq!(load_package(&link).unwrap().dir, link);
    }

    #[test]
    fn test_load_package_plain() {
        let root = TempDir::new().unwrap();
        let pkg =
            write_manifest(&root, "my-crate", &package("my-crate", "\"1.2.3-alpha.1\""));
        let info = load_package(&pkg).unwrap();
        assert_eq!(info.id.crate_name, "my-crate");
        assert_eq!(info.id.version, Version::parse("1.2.3-alpha.1").unwrap());
        assert_eq!(info.targets.len(), 1);
        assert_eq!(info.targets[0].crate_name, "my_crate");
        assert!(matches!(info.targets[0].kind, TargetKind::Lib(_)));
    }

    #[test]
    fn test_load_package_missing_version_defaults() {
        let root = TempDir::new().unwrap();
        let pkg =
            write_manifest(&root, "a", "[package]\nname = \"a\"\nedition = \"2021\"\n");
        assert_eq!(load_package(&pkg).unwrap().id.version, Version::new(0, 0, 0));
    }

    #[test]
    fn test_load_package_lib_name() {
        // The crate name comes from the target, not the package
        let root = TempDir::new().unwrap();
        let manifest = package("my-lib", "\"0.1.0\"") + "[lib]\nname = \"mylib\"\n";
        let pkg = write_manifest(&root, "my-lib", &manifest);
        let info = load_package(&pkg).unwrap();
        assert_eq!(info.id.crate_name, "my-lib");
        assert_eq!(info.targets[0].crate_name, "mylib");
    }

    #[test]
    fn test_load_package_inherited_version() {
        let root = TempDir::new().unwrap();
        write_manifest(
            &root,
            "",
            "[workspace]\nresolver = \"2\"\nmembers = [\"a\"]\n[workspace.package]\nversion = \"2.0.0\"\n",
        );
        let pkg = write_manifest(&root, "a", &package("a", "{ workspace = true }"));
        assert_eq!(load_package(&pkg).unwrap().id.version, Version::new(2, 0, 0));
    }

    #[test]
    fn test_load_package_root_is_also_workspace() {
        let root = TempDir::new().unwrap();
        let manifest = package("root", "{ workspace = true }")
            + "[workspace]\nresolver = \"2\"\n[workspace.package]\nversion = \"4.5.6\"\n";
        let pkg = write_manifest(&root, "", &manifest);
        let info = load_package(&pkg).unwrap();
        assert_eq!(info.id.crate_name, "root");
        assert_eq!(info.id.version, Version::new(4, 5, 6));
    }

    #[test]
    fn test_load_package_errors() {
        let root = TempDir::new().unwrap();
        // no Cargo.toml at all
        assert!(load_package(root.path()).is_err());
        // not a directory
        assert!(load_package(&root.path().join("missing")).is_err());
        // virtual workspace
        write_manifest(&root, "ws/a", &package("a", "\"0.1.0\""));
        let ws = write_manifest(
            &root,
            "ws",
            "[workspace]\nresolver = \"2\"\nmembers = [\"a\"]\n",
        );
        let err = load_package(&ws).unwrap_err().to_string();
        assert!(err.contains("virtual workspace"), "{err}");
    }

    #[test]
    fn test_load_members_virtual_workspace() {
        let root = TempDir::new().unwrap();
        write_manifest(
            &root,
            "",
            "[workspace]\nresolver = \"2\"\nmembers = [\"crates/*\"]\nexclude = [\"crates/skip\"]\n",
        );
        write_manifest(&root, "crates/a", &package("a", "\"0.1.0\""));
        write_manifest(&root, "crates/b", &package("b", "\"0.2.0\""));
        write_manifest(&root, "crates/skip", &package("skip", "\"0.3.0\""));
        let names: Vec<String> = load_members(root.path())
            .unwrap()
            .into_iter()
            .map(|p| p.id.crate_name)
            .sorted()
            .collect();
        assert_eq!(names, ["a", "b"]);
    }

    #[test]
    fn test_load_members_single_package() {
        let root = TempDir::new().unwrap();
        let pkg = write_manifest(&root, "a", &package("a", "\"0.1.0\""));
        let members = load_members(&pkg).unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].id.crate_name, "a");
    }
}
