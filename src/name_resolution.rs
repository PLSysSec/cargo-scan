#![allow(unused_variables)]

use super::effect::SrcLoc;
use super::ident::{CanonicalPath, Ident, Path as IdentPath};

use ra_ap_hir::db::HirDatabase;
use ra_ap_hir::{AsAssocItem, Semantics};
use ra_ap_ide::{AnalysisHost, FileId, LineCol, RootDatabase, TextSize};
use ra_ap_ide_db::base_db::SourceDatabase;
use ra_ap_ide_db::defs::{Definition, IdentClass};
use ra_ap_ide_db::FxHashMap;
use ra_ap_paths::AbsPathBuf;
use ra_ap_project_model::{
    CargoConfig, CargoFeatures, InvocationLocation, InvocationStrategy, ProjectManifest,
    ProjectWorkspace, RustcSource, UnsetTestCrates,
};
use ra_ap_rust_analyzer::cli::load_cargo::{load_workspace, LoadCargoConfig};
use ra_ap_syntax::{AstNode, SourceFile, SyntaxToken, TokenAtOffset};
use ra_ap_vfs::{Vfs, VfsPath};

use anyhow::{anyhow, Result};
use itertools::Itertools;
use std::fs::canonicalize;
use std::path::Path;

/// Path that is fully expanded and canonical
/// e.g. if I do `use libc::foobar as baz`
/// then `baz` is a valid Path, but not a valid
/// CanonicalPath
///
/// NB: `libc` and `foobar` are valid Idents and valid Paths
/// `libc::foobar` is a Path but not an Ident

/// SrcLoc: file, line, col
/// Ident: method call or function call
/// e.g. push

/// Return canonical path which that ident resolves to
/// e.g. for push we should return `std::vec::Vec::push`

pub fn resolve_path(s: SrcLoc, p: IdentPath) -> CanonicalPath {
    // let path_node =
    //     token.parent_ancestors().find(|a| a.kind() == SyntaxKind::PATH).unwrap();
    // println!("node = {:?}", path_node);
    // let syntax_path = SyntaxPath::cast(path_node).unwrap();
    // println!("syntax_path: {:?}", syntax_path.to_string());
    // let resolved_path = sems.resolve_path(&syntax_path);
    // println!("resolved path: {:?}", resolved_path);

    // if let Some(PathResolution::Def(resolved_def)) = resolved_path {
    //     let mod_path = resolved_def.canonical_path(db);
    //     println!("canon path: {:?}, {:?}", mod_path, resolved_def.module(db).unwrap().name(db).unwrap().to_string());
    //     println!();
    // }
    todo!()
}

pub struct Resolver {
    host: AnalysisHost,
    vfs: Vfs,
    // sems: Semantics<'a, RootDatabase>
}
impl Resolver {
    fn cargo_config() -> CargoConfig {
        // List of features to activate (or deactivate).
        let features = CargoFeatures::default();

        // Target triple
        let target = None;

        // Whether to load sysroot crates (`std`, `core` & friends).
        let sysroot = Some(RustcSource::Discover);

        // rustc private crate source
        let rustc_source = None;

        // crates to disable `#[cfg(test)]` on
        let unset_test_crates = UnsetTestCrates::All;

        // Setup RUSTC_WRAPPER to point to `rust-analyzer` binary itself.
        let wrap_rustc_in_build_scripts = true;

        let run_build_script_command = None;

        // Support extra environment variables via CLI:
        let extra_env = FxHashMap::default();

        let invocation_strategy = InvocationStrategy::PerWorkspace;
        let invocation_location = InvocationLocation::Workspace;

        CargoConfig {
            features,
            target,
            sysroot,
            rustc_source,
            unset_test_crates,
            wrap_rustc_in_build_scripts,
            run_build_script_command,
            extra_env,
            invocation_strategy,
            invocation_location,
        }
    }

    pub fn new(crate_path: &Path) -> Result<Resolver> {
        let canon_path = canonicalize(crate_path).unwrap();
        let abs_path = AbsPathBuf::assert(canon_path);
        // Make sure the path is a crate
        if !crate_path.is_dir() {
            return Err(anyhow!(
                "Path is not a crate; not a directory: {:?}",
                crate_path
            ));
        }

        // TODO: Maybe allow to load and analyze multiple crates
        let manifest = ProjectManifest::discover_single(&abs_path)?;
        let cargo_config = &Self::cargo_config();

        let no_progress = &|_| {};
        let ws = ProjectWorkspace::load(manifest, cargo_config, no_progress)?;

        let load_config = LoadCargoConfig {
            load_out_dirs_from_check: true,
            with_proc_macro: true,
            prefill_caches: true,
        };

        let (host, vfs, _) = load_workspace(ws, &cargo_config.extra_env, &load_config)?;

        // TODO: make db and sems fields of the Resolver
        // let db = host.raw_database();
        // let sems = Semantics::new(db);

        Ok(Resolver { host, vfs })
    }

    fn get_db(&self) -> &RootDatabase {
        self.host.raw_database()
    }

    fn get_semantics(&self) -> Semantics<RootDatabase> {
        Semantics::new(self.get_db())
    }

    fn get_file_id(
        &self,
        filepath: &Path,
        sems: &Semantics<RootDatabase>,
    ) -> Result<FileId> {
        let abs_path = canonicalize(filepath)?;
        let vfs_path = VfsPath::new_real_path(abs_path.display().to_string());

        match self.vfs.file_id(&vfs_path) {
            Some(file_id) => Ok(file_id),
            None => Err(anyhow!("The id of path {:?} does not exist in Vfs", filepath)),
        }
    }

    fn get_src_file(file_id: FileId, sems: &Semantics<RootDatabase>) -> SourceFile {
        sems.parse(file_id)
    }

    fn find_offset(&self, file_id: FileId, src_loc: SrcLoc) -> Result<TextSize> {
        // LineCol is zero-based
        let line: u32 = src_loc.start_line() as u32 - 1;
        let col: u32 = src_loc.start_col() as u32 - 1;
        let line_col = LineCol { line, col };

        let line_index = self.host.analysis().file_line_index(file_id)?;
        match line_index.offset(line_col) {
            Some(offset) => Ok(offset),
            None => Err(anyhow!(
                "Could not find offset in file for source location {:?}",
                src_loc
            )),
        }
    }

    fn get_token(
        src_file: SourceFile,
        offset: TextSize,
        ident: Ident,
    ) -> Result<SyntaxToken> {
        match src_file.syntax().clone().token_at_offset(offset) {
            TokenAtOffset::Single(t) => Ok(t),
            TokenAtOffset::Between(t1, t2) => Self::pick_best_token(t1, t2, ident),
            TokenAtOffset::None => {
                Err(anyhow!("Could not find any token at offset {:?}", offset))
            }
        }
    }

    fn pick_best_token(
        ltoken: SyntaxToken,
        rtoken: SyntaxToken,
        ident: Ident,
    ) -> Result<SyntaxToken> {
        if ltoken.to_string().eq(&ident.to_string()) {
            return Ok(ltoken);
        } else if rtoken.to_string().eq(&ident.to_string()) {
            return Ok(rtoken);
        }

        Err(anyhow!("Could not find any '{:?}' token", ident.as_str()))
    }

    fn resolve(
        &self,
        file_id: FileId,
        src_loc: SrcLoc,
        ident: Ident,
        sems: &Semantics<RootDatabase>,
        db: &RootDatabase,
    ) -> Result<CanonicalPath> {
        let offset = self.find_offset(file_id, src_loc)?;
        let src_file = Self::get_src_file(file_id, sems);
        let token = Self::get_token(src_file, offset, ident)?;

        match IdentClass::classify_token(sems, &token) {
            Some(ident) => {
                let defs = ident.definitions();
                let def = defs[0];

                let container = Self::get_container_name(db, def);
                let def_name = def.name(db).map(|name| name.to_string());
                let module = def.module(db).unwrap();

                //TODO: this is not working properly yet
                //try to exclude modules that are just block expressions,
                //that should not produce canonical paths
                if module.name(db).is_none() && !module.is_mod_rs(db) {
                    println!("There should be no canonical path for {:?}", def.name(db));
                }

                let crate_name = db.crate_graph()[module.krate().into()]
                    .display_name
                    .as_ref()
                    .map(|it| it.to_string());
                let module_path = module
                    .path_to_root(db)
                    .into_iter()
                    .rev()
                    .flat_map(|it| it.name(db).map(|name| name.to_string()));

                let cp = crate_name
                    .into_iter()
                    .chain(module_path)
                    .chain(container)
                    .chain(def_name)
                    .join("::");

                Ok(CanonicalPath::new_owned(cp))
            }
            None => Err(anyhow!("Could not classify token {:?}", token)),
        }
    }

    fn get_container_name(db: &dyn HirDatabase, def: Definition) -> Option<String> {
        match def {
            Definition::Field(f) => Some(f.parent_def(db).name(db)),
            Definition::Local(l) => l.parent(db).name(db),
            Definition::Function(f) => {
                if let Some(item) = f.as_assoc_item(db) {
                    match item.container(db) {
                        ra_ap_hir::AssocItemContainer::Trait(t) => Some(t.name(db)),
                        ra_ap_hir::AssocItemContainer::Impl(i) => {
                            i.self_ty(db).as_adt().map(|adt| adt.name(db))
                        }
                    }
                } else {
                    None
                }
            }
            Definition::Variant(e) => Some(e.parent_enum(db).name(db)),
            _ => None,
        }
        .map(|name| name.to_string())
    }

    pub fn resolve_ident(&self, s: SrcLoc, i: Ident) -> Result<CanonicalPath> {
        let db = self.get_db();
        let sems = self.get_semantics();

        let mut filepath = s.dir().clone();
        filepath.push(s.file().as_path());
        let file_id = self.get_file_id(&filepath, &sems)?;

        self.resolve(file_id, s, i, &sems, db)
    }
}
