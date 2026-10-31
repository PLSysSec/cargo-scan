//! Cargo workspace model shared by the scanner and audit chains.
//!
//! Packages and workspaces are loaded with Cargo itself, so workspace
//! inheritance, member globs, `exclude`, and target names follow Cargo's rules
//! exactly.

use crate::util::CrateId;

use anyhow::{anyhow, Result};
use cargo::compiler::{CompileKind, RustcTargetData};
use cargo::resolver::{CliFeatures, ForceAllTargets, HasDevUnits};
use cargo::util::GlobalContext;
use cargo::workspace::dependency::DepKind;
use cargo::workspace::{Package, PackageIdSpec, TargetKind, Workspace};
use clap::Args as ClapArgs;
use itertools::Itertools;
use log::debug;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::{DfsPostOrder, Reversed};
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

/// Options that control how Cargo resolves the dependency graph.
#[derive(ClapArgs, Debug, Clone, Serialize, Deserialize)]
pub struct ResolveOptions {
    /// Activate every optional Cargo feature in every package during dependency
    /// resolution (cargo --all-features). Maximises analysis coverage.
    /// Defaults to `true`.
    #[clap(long, default_value_t = true)]
    pub all_features: bool,

    /// Specific feature flags to activate (`cargo --features foo,bar/baz`).
    /// Ignored when `all_features` is `true`.
    #[clap(long, value_parser, num_args = 0..)]
    pub features: Vec<String>,

    /// Include `[dev-dependencies]` in the resolved dependency graph.
    #[clap(long, default_value_t = false)]
    pub include_dev_deps: bool,
}

impl Default for ResolveOptions {
    fn default() -> Self {
        Self { all_features: true, features: vec![], include_dev_deps: false }
    }
}

/// Handle to a package in a [`DependencyGraph`]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PackageRef(NodeIndex);

/// The resolved dependency graph of a Cargo workspace.
///
/// Only dependencies that are built for the host are included:
/// normal dependencies, and dev-dependencies if [`ResolveOptions::include_dev_deps`] is set.
/// Build dependencies are excluded.
#[derive(Debug, Clone)]
pub struct DepGraph {
    graph: DiGraph<PackageInfo, ()>,
    /// The package the workspace was loaded from, `None` for a virtual workspace
    root: Option<NodeIndex>,
}

impl DepGraph {
    /// Resolve the dependency graph of the workspace in `crate_path` with
    /// Cargo.
    ///
    /// Unlike [`load_package`], this may need the registry index and the
    /// network for dependencies that are not local paths.
    pub fn resolve(crate_path: &Path, opts: &ResolveOptions) -> Result<Self> {
        let gctx = GlobalContext::default()?;
        let ws = load_workspace(crate_path, &gctx)?;
        let specs: Vec<PackageIdSpec> =
            ws.members().map(|p| p.package_id().to_spec()).collect();

        let cli_features =
            CliFeatures::from_command_line(&opts.features, opts.all_features, true)?;
        let kinds = [CompileKind::Host];
        let mut target_data = RustcTargetData::new(&ws, &kinds)?;
        let has_dev_units =
            if opts.include_dev_deps { HasDevUnits::Yes } else { HasDevUnits::No };

        let ws_resolve = cargo::ops::resolve_ws_with_opts(
            &ws,
            &mut target_data,
            &kinds,
            &cli_features,
            &specs,
            has_dev_units,
            ForceAllTargets::No,
            true,
        )?;
        let resolve = &ws_resolve.targeted_resolve;

        let mut graph = DiGraph::new();
        let mut nodes = HashMap::new();
        for pkg_id in resolve.iter() {
            let pkg = ws_resolve.pkg_set.get_one(pkg_id)?;
            nodes.insert(pkg_id, graph.add_node(PackageInfo::from(pkg)));
        }
        for pkg_id in resolve.iter() {
            for (dep_id, deps) in resolve.deps(pkg_id) {
                // The resolve only contains activated dependencies,
                // so optional dependencies need no extra check
                let built_for_host = deps.iter().any(|dep| {
                    let wanted = match dep.kind() {
                        DepKind::Normal => true,
                        DepKind::Development => opts.include_dev_deps,
                        DepKind::Build => false,
                    };
                    wanted && target_data.dep_platform_activated(dep, CompileKind::Host)
                });
                if built_for_host {
                    graph.add_edge(nodes[&pkg_id], nodes[&dep_id], ());
                }
            }
        }

        let root = ws.current_opt().map(|p| nodes[&p.package_id()]);
        Ok(Self { graph, root })
    }

    /// The package the workspace was loaded from,
    /// `None` for a virtual workspace
    pub fn root(&self) -> Option<PackageRef> {
        self.root.map(PackageRef)
    }

    pub fn package(&self, pkg: PackageRef) -> &PackageInfo {
        &self.graph[pkg.0]
    }

    /// All packages in the graph
    pub fn packages(&self) -> impl Iterator<Item = PackageRef> + '_ {
        self.graph.node_indices().map(PackageRef)
    }

    /// Find a package by name and version
    pub fn find(&self, id: &CrateId) -> Option<PackageRef> {
        self.packages().find(|&p| self.package(p).id == *id)
    }

    /// Direct dependencies of `pkg`
    pub fn direct_dependencies(
        &self,
        pkg: PackageRef,
    ) -> impl Iterator<Item = PackageRef> + '_ {
        self.graph.neighbors_directed(pkg.0, Direction::Outgoing).map(PackageRef)
    }

    /// Returns `pkg` and everything below it (i.e., packages it transitively depends on).
    ///
    /// Bottom-up: dependencies first, `pkg` last.
    pub fn transitive_dependencies(&self, pkg: PackageRef) -> Vec<PackageRef> {
        let mut dfs = DfsPostOrder::new(&self.graph, pkg.0);
        std::iter::from_fn(|| dfs.next(&self.graph)).map(PackageRef).collect()
    }

    /// Returns `pkg` and everything above it (i.e., packages that transitively depend on it).
    ///
    /// Bottom-up: `pkg` first, top-level dependents last.
    pub fn transitive_dependents(&self, pkg: PackageRef) -> Vec<PackageRef> {
        let reversed = Reversed(&self.graph);
        let mut dfs = DfsPostOrder::new(reversed, pkg.0);
        let mut result: Vec<PackageRef> =
            std::iter::from_fn(|| dfs.next(reversed)).map(PackageRef).collect();
        result.reverse();
        result
    }
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

    /// A `root` package with one dependency of each kind, all local paths:
    /// normal `a` (which depends on `x`), dev `b`, build `c`, optional `d`
    /// behind feature `extra`, and `e` only for a target that never matches.
    fn write_dep_kinds(root: &TempDir) -> PathBuf {
        for name in ["b", "c", "d", "e", "x"] {
            write_manifest(root, name, &package(name, "\"0.1.0\""));
        }
        let a = package("a", "\"0.1.0\"") + "[dependencies]\nx = { path = \"../x\" }\n";
        write_manifest(root, "a", &a);
        let manifest = package("root", "\"0.1.0\"")
            + "[features]\nextra = [\"dep:d\"]\n"
            + "[dependencies]\n"
            + "a = { path = \"../a\" }\n"
            + "d = { path = \"../d\", optional = true }\n"
            + "[dev-dependencies]\nb = { path = \"../b\" }\n"
            + "[build-dependencies]\nc = { path = \"../c\" }\n"
            + "[target.'cfg(target_os = \"test-os\")'.dependencies]\n"
            + "e = { path = \"../e\" }\n";
        write_manifest(root, "root", &manifest)
    }

    fn names(
        graph: &DepGraph,
        pkgs: impl IntoIterator<Item = PackageRef>,
    ) -> Vec<String> {
        pkgs.into_iter().map(|p| graph.package(p).id.crate_name.clone()).collect()
    }

    fn sorted_direct_deps(graph: &DepGraph, pkg: PackageRef) -> Vec<String> {
        names(graph, graph.direct_dependencies(pkg)).into_iter().sorted().collect()
    }

    #[test]
    fn test_dep_graph_edges() {
        let root = TempDir::new().unwrap();
        let pkg = write_dep_kinds(&root);

        // Defaults: all features on, no dev-dependencies
        let graph = DepGraph::resolve(&pkg, &ResolveOptions::default()).unwrap();
        let root_pkg = graph.root().unwrap();
        assert_eq!(graph.package(root_pkg).id.crate_name, "root");
        assert_eq!(sorted_direct_deps(&graph, root_pkg), ["a", "d"]);

        // Optional `d` is not activated without its feature
        let opts = ResolveOptions { all_features: false, ..Default::default() };
        let graph = DepGraph::resolve(&pkg, &opts).unwrap();
        assert_eq!(sorted_direct_deps(&graph, graph.root().unwrap()), ["a"]);

        let opts = ResolveOptions {
            all_features: false,
            features: vec!["extra".to_string()],
            ..Default::default()
        };
        let graph = DepGraph::resolve(&pkg, &opts).unwrap();
        assert_eq!(sorted_direct_deps(&graph, graph.root().unwrap()), ["a", "d"]);

        // Dev-dependencies are included when asked for
        let opts = ResolveOptions { include_dev_deps: true, ..Default::default() };
        let graph = DepGraph::resolve(&pkg, &opts).unwrap();
        assert_eq!(sorted_direct_deps(&graph, graph.root().unwrap()), ["a", "b", "d"]);
    }

    /// A graph built directly from `(dependent, dependency)` edges
    fn graph_from_edges(names: &[&str], edges: &[(&str, &str)]) -> DepGraph {
        let mut graph = DiGraph::new();
        let nodes: HashMap<&str, NodeIndex> = names
            .iter()
            .map(|&name| {
                let id = CrateId::new(name.to_string(), Version::new(0, 1, 0));
                (
                    name,
                    graph.add_node(PackageInfo {
                        id,
                        dir: PathBuf::new(),
                        targets: vec![],
                    }),
                )
            })
            .collect();
        for (from, to) in edges {
            graph.add_edge(nodes[from], nodes[to], ());
        }
        DepGraph { graph, root: Some(nodes[names[0]]) }
    }

    #[test]
    fn test_dep_graph_order() {
        // root -> a -> x -> y, root -> b -> x, root -> x directly, and `w`
        // depends on `x` but is not reachable from `root`
        let graph = graph_from_edges(
            &["root", "a", "b", "x", "y", "w", "unrelated"],
            &[
                ("root", "a"),
                ("root", "b"),
                ("root", "x"),
                ("a", "x"),
                ("b", "x"),
                ("x", "y"),
                ("w", "x"),
            ],
        );
        let root = graph.root().unwrap();
        let x =
            graph.find(&CrateId::new("x".to_string(), Version::new(0, 1, 0))).unwrap();

        let order = graph.transitive_dependencies(root);
        assert_eq!(order.last(), Some(&root));

        let order = graph.transitive_dependents(x);
        assert_eq!(order.first(), Some(&x));
    }
}
