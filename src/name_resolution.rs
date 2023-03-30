#![allow(unused_variables)]

use std::fs::canonicalize;
use std::path::{Path, PathBuf};

use crate::ident::CanonicalPath;
use anyhow::{anyhow, Result};

use super::effect::SrcLoc;
use super::ident::{Ident, Path as IdentPath};
use ra_ap_hir::{PathResolution, Semantics};
use ra_ap_ide::{AnalysisHost, FileId, LineCol, RootDatabase, TextSize};
use ra_ap_ide_db::defs::IdentClass;
use ra_ap_paths::AbsPathBuf;
use ra_ap_project_model::{CargoConfig, ProjectManifest, ProjectWorkspace};
use ra_ap_rust_analyzer::cli::load_cargo::{load_workspace, LoadCargoConfig};

use ra_ap_syntax::{
    ast::Path as SyntaxPath, AstNode, SourceFile, SyntaxKind, SyntaxToken, TokenAtOffset,
};
use ra_ap_vfs::{Vfs, VfsPath};

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
    todo!()
}

pub struct Resolver {
    host: AnalysisHost,
    vfs: Vfs,
    // sems: Semantics<'a, RootDatabase>
}
impl Resolver {
    pub fn new(crate_path: &PathBuf, cargo_config: &CargoConfig) -> Result<Resolver> {
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

        let no_progress = &|_| {};
        let ws = ProjectWorkspace::load(manifest, cargo_config, no_progress)?;

        let load_config = LoadCargoConfig {
            load_out_dirs_from_check: true,
            with_proc_macro: false,
            prefill_caches: false,
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

        Err(anyhow!("Could not find any '{:?}' token", ident))
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

        let path_node =
            token.parent_ancestors().find(|a| a.kind() == SyntaxKind::PATH).unwrap();
        let syntax_path = SyntaxPath::cast(path_node).unwrap();
        println!("syntax_path: {:?}", syntax_path);
        println!("syntax_path: {:?}", syntax_path.coloncolon_token().unwrap().text());
        let resolved_path = sems.resolve_path(&syntax_path).unwrap();

        if let PathResolution::Def(resolved_def) = resolved_path {
            let mod_path = resolved_def.canonical_module_path(db);

            let mut canonical_path = String::new();

            match resolved_def.canonical_module_path(db) {
                Some(p) => {
                    canonical_path.push_str("crate::");
                    p.flat_map(|it| it.name(db)).for_each(|name| {
                        canonical_path.push_str(name.to_string().as_str());
                        canonical_path.push_str("::");
                    });
                    canonical_path.push_str(&resolved_def.name(db).unwrap().to_string())
                }
                None => canonical_path.push_str("No canonical path"),
            };
            println!("canonical_path: {:?}", canonical_path);
        }

        match IdentClass::classify_token(sems, &token) {
            Some(ident) => {
                let defs = ident.definitions();
                let def = defs[0];

                ///////////////////////////
                // let mod_def = match def {
                //     Definition::Function(it) => ModuleDef::from(it),
                //     Definition::Macro(it) => ModuleDef::from(it),
                //     Definition::Module(it) => ModuleDef::from(it),
                //     Definition::Adt(it) => ModuleDef::from(it),
                //     Definition::Variant(it) => ModuleDef::from(it),
                //     Definition::Const(it) => ModuleDef::from(it),
                //     Definition::Static(it) => ModuleDef::from(it),
                //     Definition::Trait(it) => ModuleDef::from(it),
                //     Definition::TypeAlias(it) => ModuleDef::from(it),
                //     Definition::BuiltinType(it) => ModuleDef::from(it),
                //     Definition::Field(_) => todo!(),
                //     Definition::SelfType(_) => todo!(),
                //     Definition::Local(_) => todo!(),
                //     Definition::GenericParam(_) => todo!(),
                //     Definition::Label(_) => todo!(),
                //     Definition::DeriveHelper(_) => todo!(),
                //     Definition::BuiltinAttr(_) => todo!(),
                //     Definition::ToolModule(_) => todo!(),

                // };
                // println!("mod def {:?}", mod_def.canonical_path(db));
                //////////////////////////

                let mut canonical_path = String::new();

                match def.canonical_module_path(db) {
                    Some(p) => {
                        canonical_path.push_str("crate::");
                        p.flat_map(|it| it.name(db)).for_each(|name| {
                            canonical_path.push_str(name.to_string().as_str());
                            canonical_path.push_str("::");
                        });
                        canonical_path.push_str(&def.name(db).unwrap().to_string())
                    }
                    None => canonical_path.push_str("No canonical path"),
                };
                Ok(CanonicalPath::new_owned(canonical_path))
            }
            None => Err(anyhow!("Could not classify token {:?}", token)),
        }
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
