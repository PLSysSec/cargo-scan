use anyhow::{anyhow, Result};
use itertools::Itertools;
use log::debug;
use ra_ap_cfg::CfgDiff;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::canonicalize;
use std::path::Path;
use std::sync::Arc;

use crate::effect::SrcLoc;
use crate::ident::{CanonicalPath, CanonicalType, Ident};
use crate::offset_maping::MacroExpansionContext;

use ra_ap_hir::{AssocItem, CfgAtom, Crate, HirFileId, Impl, Semantics};
use ra_ap_hir_def::db::DefDatabase;
use ra_ap_hir_def::{FunctionId, Lookup};
use ra_ap_ide::{AnalysisHost, Diagnostic, FileId, LineCol, LineIndex, RootDatabase, TextSize};
use ra_ap_ide_db::base_db::Upcast;
use ra_ap_ide_db::defs::{Definition, IdentClass};
use ra_ap_ide_db::FxHashMap;
use ra_ap_load_cargo::{LoadCargoConfig, ProcMacroServerChoice};
use ra_ap_project_model::{
    CargoConfig, CargoFeatures, CfgOverrides, InvocationLocation, InvocationStrategy,
    RustLibSource,
};
use ra_ap_syntax::{SyntaxNode, SyntaxToken};
use ra_ap_vfs::{Vfs, VfsPath};

use super::util::{canonical_path, get_canonical_type, get_token, syntax_node_from_def};

#[derive(Debug)]
pub struct Resolver {
    host: AnalysisHost,
    vfs: Vfs,
    pub macro_file_line_index: RefCell<FxHashMap<HirFileId, Arc<LineIndex>>>,
    macro_expansion_ctx: RefCell<Option<MacroExpansionContext>>,
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

        Ok(Resolver { host, vfs, macro_file_line_index: RefCell::new(FxHashMap::default()),macro_expansion_ctx: RefCell::new(None),})
    }

    pub fn db(&self) -> &RootDatabase {
        self.host.raw_database()
    }

    pub fn add_file(&mut self, path: &Path, content: String) -> Result<FileId> {
        let abs_path = canonicalize(path)?;
        let vfs_path = VfsPath::new_real_path(abs_path.display().to_string());
        
        self.vfs.set_file_contents(vfs_path.clone(), Some(content.into()));
        self.vfs.file_id(&vfs_path)
            .ok_or_else(|| anyhow!("FileId not found after insertion"))
    }


    pub fn find_file_id(&self, filepath: &Path) -> Result<FileId> {
        let abs_path = canonicalize(filepath)?;
        let vfs_path = VfsPath::new_real_path(abs_path.display().to_string());

        match self.vfs.file_id(&vfs_path) {
            Some(file_id) => Ok(file_id),
            None => Err(anyhow!("The id of path {:?} does not exist in Vfs", filepath)),
        }
    }

    fn find_offset(&self, file_id: HirFileId, src_loc: SrcLoc) -> Result<TextSize> {
        // LineCol is zero-based
        let line: u32 = src_loc.start_line() as u32 - 1;
        let col: u32 = src_loc.start_col() as u32 - 1;
        let line_col = LineCol { line, col };
        if let Some(ctx) = self.current_macro_expansion_ctx() {
            let offset = ctx.line_index.offset(line_col)
                .ok_or_else(|| anyhow!("LineIndex out of bounds"))?;
            ctx.offset_mapping.to_raw_offset(offset.into())
                .ok_or_else(|| anyhow!("OffsetMapping failed to map formatted offset"))    
        } else if file_id.is_macro() {
            let map_ref = self.macro_file_line_index.borrow();
            let line_index = map_ref.get(&file_id).ok_or_else(|| {
                anyhow!("LineIndex not found for macro-file: {:?}", file_id)
            })?;
            line_index.offset(line_col).ok_or_else(|| anyhow!("Could not find offset for macro"))
        } else {
            let original_file_id = file_id.file_id().unwrap();
            let line_index = self.host.analysis().file_line_index(original_file_id)?;
            line_index.offset(line_col).ok_or_else(|| anyhow!("Could not find offset in normal file"))
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

    pub fn set_macro_expansion_ctx(&self, ctx: MacroExpansionContext) {
        self.macro_expansion_ctx.replace(Some(ctx));
    }

    pub fn clear_macro_expansion_ctx(&self) {
        self.macro_expansion_ctx.replace(None);
    }

    pub fn current_macro_expansion_ctx(&self) -> Option<MacroExpansionContext> {
        self.macro_expansion_ctx.borrow().clone()
    }
}

/// Core API for the name resolution of Rust identifiers.
/// ResolverImpl is used to perform the actual semantic queries.
/// A new instance is created every time we parse a new file in Scanner.
#[derive(Debug)]
pub struct ResolverImpl<'a> {
    db: &'a RootDatabase,
    pub sems: Semantics<'a, RootDatabase>,
    resolver: &'a Resolver,
    /// The syntax tree of the file
    /// we are currently scanning
    src_file: SyntaxNode,
    file_id: HirFileId,
    /// Set of diagnostics for the given file
    file_diags: Vec<Diagnostic>,
}

impl<'a> ResolverImpl<'a> {
    pub fn new(resolver: &'a Resolver, file_id: HirFileId) -> Result<Self> {
        let db = resolver.db();
        let sems = Semantics::new(db);
        let src_file = sems.parse_or_expand(file_id);

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

    pub fn update_macro_file_line_index(&self, file_id: HirFileId,expanded_syntax:SyntaxNode) -> anyhow::Result<()> {
        if !file_id.is_macro() {
            return Ok(());
        }
        if self.resolver.macro_file_line_index.borrow().contains_key(&file_id) {
            return Ok(());
        }
        let text = expanded_syntax.text().to_string();
        let line_index = Arc::new(LineIndex::new(&text));
        self.resolver.macro_file_line_index.borrow_mut().insert(file_id, line_index);
        Ok(())
    }

    fn parse_source_file(&self, def: &Definition) -> Option<()> {
        let db = self.db;
        let node = syntax_node_from_def(def, db)?;
        // If it does not have a `FileId`, then it was produced
        // by a macro call and we want to skip those cases.
        self.sems.parse_or_expand(node.file_id);

        Some(())
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
        let offset = match self.resolver.find_offset(self.file_id, s) {
            Ok(o) => o,
            Err(e) => {
                return Err(e);
            }
        };
        let tok = get_token(&self.src_file, offset, i)?;
        Ok(tok)
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
        if i.to_string() == "dummy" {
            return Err(anyhow!("Skipped resolving 'dummy'"));
        }
        let token = self.token(i, s)?;
        let def = self.find_def(&token)?;
        self.parse_source_file(&def);

        canonical_path(&self.sems, self.db, &def)
            .ok_or_else(|| anyhow!("Could not construct canonical path for '{:?}'", def))
    }

    pub fn resolve_type(&self, s: SrcLoc, i: Ident) -> Result<CanonicalType> {
        if i.to_string() == "dummy" {
            return Err(anyhow!("Skipped resolving 'dummy'"));
        }
        let token = self.token(i, s)?;
        let def = self.find_def(&token)?;

        get_canonical_type(self.db, &def)
    }

    pub fn is_ffi(&self, s: SrcLoc, i: Ident) -> Result<bool> {
        if i.to_string() == "dummy" {
            return Err(anyhow!("Skipped resolving 'dummy'"));
        }
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
        if i.to_string() == "dummy" {
            return Err(anyhow!("Skipped resolving 'dummy'"));
        }
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
        if i.to_string() == "dummy" {
            return Err(anyhow!("Skipped resolving 'dummy'"));
        }
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
        if i.to_string() == "dummy" {
            return Err(anyhow!("Skipped resolving 'dummy'"));
        }
        let token = self.token(i.clone(), s.clone())?;
        let def = self.find_def(&token)?;

        let mut impl_methods_for_trait_method: Vec<CanonicalPath> = Vec::new();
        let filter_ = |x: AssocItem| match x {
            AssocItem::Function(f) => {
                self.parse_source_file(&f.into())?;
                canonical_path(&self.sems, self.db, &f.into())
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
