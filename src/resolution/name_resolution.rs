use anyhow::{anyhow, Result};
use itertools::Itertools;
use log::debug;
use ra_ap_cfg::CfgDiff;
use std::collections::HashMap;
use std::fs::canonicalize;
use std::path::Path;

use crate::effect::SrcLoc;
use crate::ident::{CanonicalPath, CanonicalType, Ident};

use ra_ap_hir::{AssocItem, CfgAtom, Crate, Impl, Semantics};
use ra_ap_hir_def::db::DefDatabase;
use ra_ap_hir_def::{FunctionId, Lookup};
use ra_ap_ide::{AnalysisHost, Diagnostic, FileId, LineCol, RootDatabase, TextSize};
use ra_ap_ide_db::base_db::Upcast;
use ra_ap_ide_db::defs::{Definition, IdentClass};
use ra_ap_ide_db::FxHashMap;
use ra_ap_load_cargo::{LoadCargoConfig, ProcMacroServerChoice};
use ra_ap_project_model::{
    CargoConfig, CargoFeatures, CfgOverrides, InvocationLocation, InvocationStrategy,
    RustLibSource,
};
use ra_ap_syntax::{SourceFile, SyntaxToken};
use ra_ap_vfs::{Vfs, VfsPath};

use super::util::{canonical_path, get_canonical_type, get_token, syntax_node_from_def};

#[derive(Debug)]
pub struct Resolver {
    host: AnalysisHost,
    vfs: Vfs,
}

impl Resolver {
    fn cargo_config() -> CargoConfig {
        // List of features to activate (or deactivate).
        let features = CargoFeatures::All;

        // Target triple
        let target = None;

        // Whether to load sysroot crates
        let sysroot = Some(RustLibSource::Discover);

        // rustc private crate source
        let rustc_source = None;

        // Setup RUSTC_WRAPPER to point to `rust-analyzer` binary itself.
        let wrap_rustc_in_build_scripts = true;

        let run_build_script_command = None;

        // Support extra environment variables via CLI:
        let extra_env = FxHashMap::default();

        let invocation_strategy = InvocationStrategy::PerWorkspace;
        let invocation_location = InvocationLocation::Workspace;

        let sysroot_src = None;
        let extra_args = Vec::new();

        // Disable '#[cfg(test)]' in all crates of the workspace
        let disabled_cfgs =
            CfgDiff::new(vec![], vec![CfgAtom::Flag("test".into())]).unwrap_or_default();
        let cfg_overrides =
            CfgOverrides { global: disabled_cfgs, selective: Default::default() };

        CargoConfig {
            features,
            target,
            sysroot,
            sysroot_src,
            rustc_source,
            cfg_overrides,
            wrap_rustc_in_build_scripts,
            run_build_script_command,
            extra_args,
            extra_env,
            invocation_strategy,
            invocation_location,
            target_dir: None,
        }
    }

    pub fn new(crate_path: &Path) -> Result<Resolver> {
        debug!("Creating resolver with path {:?}", crate_path);

        // Make sure the path is a crate
        if !crate_path.is_dir() {
            return Err(anyhow!(
                "Path is not a crate; not a directory: {:?}",
                crate_path
            ));
        }

        // TODO: Maybe allow to load and analyze multiple workspaces
        let cargo_config = &Self::cargo_config();
        let progress = &|p| debug!("Workspace loading progress: {:?}", p);

        let with_proc_macro_server = ProcMacroServerChoice::Sysroot;

        let load_config = LoadCargoConfig {
            load_out_dirs_from_check: true,
            with_proc_macro_server,
            prefill_caches: true,
        };

        let (host, vfs, _) = ra_ap_load_cargo::load_workspace_at(
            crate_path,
            cargo_config,
            &load_config,
            progress,
        )?;

        debug!("...created");

        Ok(Resolver { host, vfs })
    }

    fn db(&self) -> &RootDatabase {
        self.host.raw_database()
    }

    fn find_file_id(&self, filepath: &Path) -> Result<FileId> {
        let abs_path = canonicalize(filepath)?;
        let vfs_path = VfsPath::new_real_path(abs_path.display().to_string());

        match self.vfs.file_id(&vfs_path) {
            Some(file_id) => Ok(file_id),
            None => Err(anyhow!("The id of path {:?} does not exist in Vfs", filepath)),
        }
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

    pub fn get_cfg_options_for_crate(
        &self,
        name: &String,
    ) -> Result<HashMap<String, Vec<String>>> {
        let db = self.db();
        let mut crate_opts: HashMap<String, Vec<String>> = HashMap::default();

        let crate_ = Crate::all(db).into_iter().find(|x| match x.display_name(db) {
            Some(crate_name) => name.eq(&crate_name.to_string()),
            None => false,
        });

        if let Some(crate_) = crate_ {
            let enabled_opts = crate_.cfg(db);
            for key in enabled_opts.get_cfg_keys() {
                let cfg_values = enabled_opts.get_cfg_values(key).map(|x| x.to_string());
                crate_opts.insert(key.to_string(), Vec::from_iter(cfg_values));
            }
        } else {
            return Err(anyhow!("Could not get cfg options for crate: {:?}", name));
        }

        Ok(crate_opts)
    }
}

/// Core API for the name resolution of Rust identifiers.
/// ResolverImpl is used to perform the actual semantic queries.
/// A new instance is created every time we parse a new file in Scanner.
#[derive(Debug)]
pub struct ResolverImpl<'a> {
    db: &'a RootDatabase,
    sems: Semantics<'a, RootDatabase>,
    resolver: &'a Resolver,
    /// The syntax tree of the file
    /// we are currently scanning
    src_file: SourceFile,
    file_id: FileId,
    /// Set of diagnostics for the given file
    file_diags: Vec<Diagnostic>,
}

impl<'a> ResolverImpl<'a> {
    pub fn new(resolver: &'a Resolver, filepath: &Path) -> Result<Self> {
        let db = resolver.db();
        let sems = Semantics::new(db);
        let file_id = resolver.find_file_id(filepath)?;
        let src_file = sems.parse(file_id);

        // TBD: This causes a stack overflow on some crates
        // Disabling until a fix is found, can re-enable if needed for
        // individual runs
        // let file_diags = resolver.host.analysis().diagnostics(
        //     &ra_ap_ide::DiagnosticsConfig::test_sample(),
        //     ra_ap_ide::AssistResolveStrategy::None,
        //     file_id,
        // )?;
        let file_diags = Vec::new();

        Ok(ResolverImpl { db, sems, resolver, src_file, file_id, file_diags })
    }

    fn def_source_loc(&self, def: &Definition) -> Option<SrcLoc> {
        let db = self.db;
        let node = syntax_node_from_def(def, db)?;

        // If it does not have a `FileId`, then it was produced
        // by a macro call and we want to skip those cases.
        let file_id = node.file_id.file_id()?;
        self.sems.parse(file_id);

        let range = self.sems.original_range(&node.value).range;
        let vfs_path = self.resolver.vfs.file_path(file_id);
        let str_path = vfs_path.to_string();
        let filepath = Path::new(&str_path);

        let line_index = self.resolver.host.analysis().file_line_index(file_id).ok()?;
        // LineCol is zero-based in RA
        let start_line_col = line_index.line_col(range.start());
        let end_line_col = line_index.line_col(range.end());
        let src_loc = SrcLoc::new(
            filepath,
            start_line_col.line as usize + 1,
            start_line_col.col as usize + 1,
            end_line_col.line as usize + 1,
            end_line_col.col as usize + 1,
        );

        Some(src_loc)
    }

    fn find_def(&self, token: &SyntaxToken) -> Result<Definition> {
        // For ra_ap_syntax::TextSize, using default, idk if this is correct
        let text_size = Default::default();
        self.sems
            .descend_into_macros(token.clone(), text_size)
            .iter()
            .filter_map(|t| {
                // 'IdentClass::classify_token' might return two definitions
                // due to field shorthand syntax, which uses a single
                // reference to point to two different defs.
                // We only care about the first one, which corresponds to a
                // local definition.
                IdentClass::classify_token(&self.sems, t)?.definitions().pop_at(0)
            })
            .exactly_one()
            .or(Err(anyhow!(
                "Could not classify token {:?}. Diagnostics: {:?}",
                token.to_string(),
                self.get_token_diagnostics(token)
            )))
    }

    fn token(&self, i: Ident, s: SrcLoc) -> Result<SyntaxToken> {
        let offset = self.resolver.find_offset(self.file_id, s)?;
        get_token(&self.src_file, offset, i)
    }

    fn get_token_diagnostics(&self, token: &SyntaxToken) -> Vec<String> {
        self.file_diags
            .iter()
            .filter(|d| {
                d.range.contains_range(token.text_range())
                    || d.code.as_str().eq_ignore_ascii_case("unlinked-file")
            })
            .map(|d| d.code.as_str().to_string())
            .collect_vec()
    }

    pub fn resolve_ident(&self, s: SrcLoc, i: Ident) -> Result<CanonicalPath> {
        let token = self.token(i, s.clone())?;
        let def = self.find_def(&token)?;
        let def_loc = self.def_source_loc(&def);

        canonical_path(&self.sems, self.db, &def)
            .ok_or_else(|| anyhow!("Could not construct canonical path for '{:?}'", def))
            .map(|mut cp| match def_loc {
                Some(loc) => cp.add_src_loc(loc),
                None => cp.add_src_loc(s),
            })
    }

    pub fn resolve_type(&self, s: SrcLoc, i: Ident) -> Result<CanonicalType> {
        let token = self.token(i, s)?;
        let def = self.find_def(&token)?;

        get_canonical_type(self.db, &def)
    }

    pub fn is_ffi(&self, s: SrcLoc, i: Ident) -> Result<bool> {
        let token = self.token(i, s)?;
        let def = self.find_def(&token)?;

        match def {
            Definition::Function(function) => {
                let func_id = FunctionId::from(function);

                match func_id.lookup(self.db.upcast()).container {
                    // Function is in an `extern` block.
                    ra_ap_hir_def::ItemContainerId::ExternBlockId(_) => Ok(true),
                    _ => Ok(false),
                }
            }
            Definition::Static(st) => {
                let static_id = ra_ap_hir_def::StaticId::from(st);
                match static_id.lookup(self.db).container {
                    // Static variable is in an 'extern' block
                    ra_ap_hir_def::ItemContainerId::ExternBlockId(_) => Ok(true),
                    _ => Ok(false),
                }
            }
            _ => Ok(false),
        }
    }

    pub fn is_unsafe_call(&self, s: SrcLoc, i: Ident) -> Result<bool> {
        let token = self.token(i, s)?;
        let def = self.find_def(&token)?;

        if let Definition::Function(f) = def {
            let func_id = FunctionId::from(f);
            let data = self.db.function_data(func_id);
            Ok(data.has_unsafe_kw())
        } else {
            Ok(false)
        }
    }

    pub fn is_const_or_immutable_static_ident(
        &self,
        s: SrcLoc,
        i: Ident,
    ) -> Result<bool> {
        let token = self.token(i, s)?;
        let def = self.find_def(&token)?;

        let is_immutable_static = |def: Definition| -> bool {
            if let Definition::Static(s) = def {
                return !s.is_mut(self.db);
            }
            false
        };

        Ok(matches!(def, Definition::Const(_)) || is_immutable_static(def))
    }

    /// Gathers all the implementations
    /// for all methods of the input trait
    pub fn all_impl_methods_for_trait(
        &self,
        s: SrcLoc,
        i: Ident,
    ) -> Result<Vec<CanonicalPath>> {
        let token = self.token(i.clone(), s.clone())?;
        let def = self.find_def(&token)?;

        let mut impl_methods_for_trait_method: Vec<CanonicalPath> = Vec::new();
        let filter_ = |x: AssocItem| match x {
            AssocItem::Function(f) => {
                let cp = canonical_path(&self.sems, self.db, &Definition::from(f));
                let def_loc = self.def_source_loc(&f.into())?;
                cp.map(|mut cp| cp.add_src_loc(def_loc))
            }
            _ => None,
        };

        if let Definition::Trait(tr) = def {
            for imp in Impl::all_for_trait(self.db, tr).iter() {
                imp.items(self.db)
                    .iter()
                    .filter_map(|&i| filter_(i))
                    .for_each(|x| impl_methods_for_trait_method.push(x))
            }
        } else {
            return Err(anyhow!("No trait definition found for token {:?}. Can not look for impl methods.", i.to_string()));
        }

        Ok(impl_methods_for_trait_method)
    }
}
