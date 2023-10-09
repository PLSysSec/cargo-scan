#![allow(unused_variables)]

use anyhow::{anyhow, Result};
use itertools::Itertools;
use log::debug;
use ra_ap_cfg::CfgDiff;
use ra_ap_hir_expand::InFile;
use std::collections::HashMap;
use std::fs::canonicalize;
use std::path::Path;

use super::effect::SrcLoc;
use super::ident::{CanonicalPath, CanonicalType, Ident, TypeKind};

use ra_ap_hir::{
    Adt, AsAssocItem, AssocItem, AssocItemContainer, CfgAtom, Crate, DefWithBody,
    GenericParam, HasSource, HirDisplay, Impl, Module, ModuleSource, Semantics, TypeRef,
    VariantDef,
};
use ra_ap_hir_def::db::DefDatabase;
use ra_ap_hir_def::{FunctionId, Lookup, TypeAliasId};
use ra_ap_hir_expand::name::AsName;
use ra_ap_hir_ty::db::HirDatabase;
use ra_ap_hir_ty::{Interner, TyKind};
use ra_ap_ide::{AnalysisHost, FileId, LineCol, RootDatabase, TextSize};
use ra_ap_ide_db::base_db::{SourceDatabase, Upcast};
use ra_ap_ide_db::defs::{Definition, IdentClass};
use ra_ap_ide_db::FxHashMap;
use ra_ap_load_cargo::{LoadCargoConfig, ProcMacroServerChoice};
use ra_ap_project_model::{
    CargoConfig, CargoFeatures, CfgOverrides, InvocationLocation, InvocationStrategy,
    RustLibSource,
};
use ra_ap_syntax::ast::HasName;
use ra_ap_syntax::{AstNode, SourceFile, SyntaxNode, SyntaxToken, TokenAtOffset};
use ra_ap_vfs::{Vfs, VfsPath};

// latest rust-analyzer has removed Display for Name, see
// https://docs.rs/ra_ap_hir/latest/ra_ap_hir/struct.Name.html#
// This is a wrapper function to recover the .to_string() implementation
fn name_to_string(n: ra_ap_hir::Name) -> String {
    n.to_smol_str().to_string()
}

#[derive(Debug)]
pub struct Resolver {
    host: AnalysisHost,
    vfs: Vfs,
    // sems: Semantics<'a, RootDatabase>
}
impl Resolver {
    fn cargo_config() -> CargoConfig {
        // List of features to activate (or deactivate).
        let features = CargoFeatures::All;

        // Target triple
        let target = None;

        // Whether to load sysroot crates (`std`, `core` & friends).
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

        // TODO: Maybe allow to load and analyze multiple crates
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

        // TODO: make db and sems fields of the Resolver
        // let db = host.raw_database();
        // let sems = Semantics::new(db);

        debug!("...created");

        Ok(Resolver { host, vfs })
    }

    fn get_db(&self) -> &RootDatabase {
        self.host.raw_database()
    }

    fn get_semantics(&self) -> Semantics<RootDatabase> {
        Semantics::new(self.get_db())
    }

    fn get_file_id(&self, s: &SrcLoc, sems: &Semantics<RootDatabase>) -> Result<FileId> {
        let mut filepath = s.dir().clone();
        filepath.push(s.file().as_path());
        let abs_path = canonicalize(filepath.clone())?;
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

    fn syntax_node_from_def(
        &self,
        def: &Definition,
        db: &RootDatabase,
        sems: &Semantics<RootDatabase>,
    ) -> Option<InFile<SyntaxNode>> {
        match def {
            Definition::Function(x) => x.source(db)?.syntax().original_syntax_node(db),
            Definition::Adt(x) => x.source(db)?.syntax().original_syntax_node(db),
            Definition::Variant(x) => x.source(db)?.syntax().original_syntax_node(db),
            Definition::Const(x) => x.source(db)?.syntax().original_syntax_node(db),
            Definition::Static(x) => x.source(db)?.syntax().original_syntax_node(db),
            Definition::Trait(x) => x.source(db)?.syntax().original_syntax_node(db),
            Definition::TraitAlias(x) => x.source(db)?.syntax().original_syntax_node(db),
            Definition::TypeAlias(x) => x.source(db)?.syntax().original_syntax_node(db),
            Definition::SelfType(x) => x.source(db)?.syntax().original_syntax_node(db),
            Definition::Local(x) => {
                x.primary_source(db).source(db)?.syntax().original_syntax_node(db)
            }
            Definition::Label(x) => x.source(db).syntax().original_syntax_node(db),
            Definition::ExternCrateDecl(x) => {
                x.source(db)?.syntax().original_syntax_node(db)
            }
            _ => None,
        }
    }

    fn def_source_loc(
        &self,
        def: &Definition,
        db: &RootDatabase,
        sems: &Semantics<RootDatabase>,
    ) -> Option<SrcLoc> {
        let node = self.syntax_node_from_def(def, db, sems)?;

        // If it does not have a `FileId`, then it was produced
        // by a macro call and we want to skip those cases.
        let file_id = node.file_id.file_id()?;
        sems.parse(file_id);
        let range = sems.original_range(&node.value).range;
        let vfs_path = self.vfs.file_path(file_id);
        let str_path = vfs_path.to_string();
        let filepath = Path::new(&str_path);

        let line_index = self.host.analysis().file_line_index(file_id).ok()?;
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

    fn token(
        &self,
        sems: &Semantics<RootDatabase>,
        file_id: FileId,
        i: Ident,
        s: SrcLoc,
    ) -> Result<SyntaxToken> {
        let offset = self.find_offset(file_id, s)?;
        let src_file = sems.parse(file_id);

        get_token(src_file, offset, i)
    }

    pub fn resolve_ident(&self, s: SrcLoc, i: Ident) -> Result<CanonicalPath> {
        let db = self.get_db();
        let sems = self.get_semantics();
        let file_id = self.get_file_id(&s, &sems)?;
        let token = self.token(&sems, file_id, i, s.clone())?;
        let diags = self.get_ra_diagnostics(file_id, &token);
        let def = find_def(&token, &sems, db, diags)?;
        let def_loc = self.def_source_loc(&def, db, &sems);

        canonical_path(&sems, db, &def)
            .ok_or_else(|| anyhow!("Could not construct canonical path for '{:?}'", def))
            .map(|mut cp| match def_loc {
                Some(loc) => cp.add_src_loc(loc),
                None => cp.add_src_loc(s),
            })
    }

    pub fn resolve_type(&self, s: SrcLoc, i: Ident) -> Result<CanonicalType> {
        let db = self.get_db();
        let sems = self.get_semantics();
        let file_id = self.get_file_id(&s, &sems)?;
        let token = self.token(&sems, file_id, i, s)?;
        let diags = self.get_ra_diagnostics(file_id, &token);
        let def = find_def(&token, &sems, db, diags)?;

        get_canonical_type(&sems, db, &def)
    }

    pub fn is_ffi(&self, s: SrcLoc, i: Ident) -> Result<bool> {
        let db = self.get_db();
        let sems = self.get_semantics();
        let file_id = self.get_file_id(&s, &sems)?;
        let token = self.token(&sems, file_id, i, s)?;
        let diags = self.get_ra_diagnostics(file_id, &token);
        let def = find_def(&token, &sems, db, diags)?;

        match def {
            Definition::Function(function) => {
                let func_id = FunctionId::from(function);
                let data = db.function_data(func_id);

                match func_id.lookup(db.upcast()).container {
                    // Function is in an `extern` block.
                    ra_ap_hir_def::ItemContainerId::ExternBlockId(_) => Ok(true),
                    _ => Ok(false),
                }
            }
            Definition::Static(st) => {
                let static_id = ra_ap_hir_def::StaticId::from(st);
                match static_id.lookup(db).container {
                    // Static variable is in an 'extern' block
                    ra_ap_hir_def::ItemContainerId::ExternBlockId(_) => Ok(true),
                    _ => Ok(false),
                }
            }
            _ => Ok(false),
        }
    }

    pub fn is_unsafe_call(&self, s: SrcLoc, i: Ident) -> Result<bool> {
        let db = self.get_db();
        let sems = self.get_semantics();
        let file_id = self.get_file_id(&s, &sems)?;
        let token = self.token(&sems, file_id, i, s)?;
        let diags = self.get_ra_diagnostics(file_id, &token);
        let def = find_def(&token, &sems, db, diags)?;

        if let Definition::Function(f) = def {
            let func_id = FunctionId::from(f);
            let data = db.function_data(func_id);
            Ok(data.has_unsafe_kw())
        } else {
            // Not necessary for v0, as we do not always have enough
            // information for function pointers and closures
            // is_unsafe_callable_ty(def, db)
            Ok(false)
        }
    }

    fn get_ra_diagnostics(&self, file_id: FileId, token: &SyntaxToken) -> Vec<String> {
        let diags = self.host.analysis().diagnostics(
            &ra_ap_ide::DiagnosticsConfig::test_sample(),
            ra_ap_ide::AssistResolveStrategy::None,
            file_id,
        );

        diags
            .unwrap_or(Vec::new())
            .iter()
            .filter(|d| {
                d.range.contains_range(token.text_range())
                    || d.code.as_str().eq_ignore_ascii_case("unlinked-file")
            })
            .map(|d| d.code.as_str().to_string())
            .collect_vec()
    }

    pub fn is_const_or_immutable_static_ident(
        &self,
        s: SrcLoc,
        i: Ident,
    ) -> Result<bool> {
        let db = self.get_db();
        let sems = self.get_semantics();
        let file_id = self.get_file_id(&s, &sems)?;
        let token = self.token(&sems, file_id, i, s)?;
        let diags = self.get_ra_diagnostics(file_id, &token);
        let def = find_def(&token, &sems, db, diags)?;

        let is_immutable_static = |def: Definition| -> bool {
            if let Definition::Static(s) = def {
                return !s.is_mut(db);
            }
            false
        };

        Ok(matches!(def, Definition::Const(_)) || is_immutable_static(def))
    }

    pub fn all_impl_methods_for_trait_method(
        &self,
        s: SrcLoc,
        i: Ident,
        m: String,
    ) -> Result<Vec<CanonicalPath>> {
        let db = self.get_db();
        let sems = self.get_semantics();
        let file_id = self.get_file_id(&s, &sems)?;
        let token = self.token(&sems, file_id, i.clone(), s.clone())?;
        let diags = self.get_ra_diagnostics(file_id, &token);
        let def = find_def(&token, &sems, db, diags)?;

        let mut impl_methods_for_trait_method: Vec<CanonicalPath> = Vec::new();
        let filter_ = |x: AssocItem| match x {
            AssocItem::Function(f) if name_to_string(f.name(db)) == m => {
                let cp = canonical_path(&sems, db, &Definition::from(f));
                let def_loc = self.def_source_loc(&f.into(), db, &sems)?;
                cp.map(|mut cp| cp.add_src_loc(def_loc))
            }
            _ => None,
        };

        if let Definition::Trait(tr) = def {
            Impl::all_for_trait(db, tr).iter().for_each(|impl_| {
                impl_
                    .items(db)
                    .iter()
                    .filter_map(|&i| filter_(i))
                    .for_each(|x| impl_methods_for_trait_method.push(x))
            });
        } else {
            return Err(anyhow!("No trait definition found for token {:?}. Can not look for impl methods.", i.to_string()));
        }

        Ok(impl_methods_for_trait_method)
    }

    pub fn get_cfg_options_for_crate(
        &self,
        name: &String,
    ) -> Result<HashMap<String, Vec<String>>> {
        let db = self.get_db();
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

fn get_token(
    src_file: SourceFile,
    offset: TextSize,
    ident: Ident,
) -> Result<SyntaxToken> {
    match src_file.syntax().token_at_offset(offset) {
        TokenAtOffset::Single(t) => Ok(t),
        TokenAtOffset::Between(t1, t2) => pick_best_token(t1, t2, ident),
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

fn find_def(
    token: &SyntaxToken,
    sems: &Semantics<RootDatabase>,
    db: &RootDatabase,
    diagnostics: Vec<String>,
) -> Result<Definition> {
    // For ra_ap_syntax::TextSize, using default, idk if this is correct
    let text_size = Default::default();
    sems.descend_into_macros(token.clone(), text_size)
        .iter()
        .filter_map(|t| {
            // 'IdentClass::classify_token' might return two definitions
            // due to field shorthand syntax, which uses a single
            // reference to point to two different defs.
            // We only care about the first one, which corresponds to a
            // local definition.
            IdentClass::classify_token(sems, t)?.definitions().pop_at(0)
        })
        .exactly_one()
        .or(Err(anyhow!(
            "Could not classify token {:?}. Diagnostics: {:?}",
            token.to_string(),
            diagnostics
        )))
}

fn build_path_to_root(module: Module, db: &RootDatabase) -> Vec<Module> {
    let mut path = vec![module];
    let mut curr = module;
    while let Some(next) = curr.parent(db) {
        path.push(next);
        curr = next
    }

    if let Some(module_id) = ra_ap_hir_def::ModuleId::from(curr).containing_module(db) {
        let mut parent_path = build_path_to_root(module_id.into(), db);
        path.append(&mut parent_path);
    }

    path
}

fn canonical_path(
    sems: &Semantics<RootDatabase>,
    db: &RootDatabase,
    def: &Definition,
) -> Option<CanonicalPath> {
    if let Definition::BuiltinType(b) = def {
        return Some(CanonicalPath::new(name_to_string(b.name()).as_str()));
    }

    let container = get_container_name(sems, db, def);
    let def_name = def.name(db).map(name_to_string);
    let module = def.module(db)?;

    let crate_name = db.crate_graph()[module.krate().into()]
        .display_name
        .as_ref()
        .map(|it| it.to_string());
    let module_path = build_path_to_root(module, db)
        .into_iter()
        .rev()
        .flat_map(|it| it.name(db).map(name_to_string));

    let cp = crate_name
        .into_iter()
        .chain(module_path)
        .chain(container)
        .chain(def_name)
        .join("::");

    Some(CanonicalPath::new(cp.as_str()))
}

fn get_container_name(
    sems: &Semantics<RootDatabase>,
    db: &RootDatabase,
    def: &Definition,
) -> Vec<String> {
    let mut container_names = vec![];

    match def {
        Definition::Field(f) => {
            let parent = f.parent_def(db);
            container_names.append(&mut match parent {
                VariantDef::Variant(v) => get_container_name(sems, db, &v.into()),
                VariantDef::Struct(s) => {
                    get_container_name(sems, db, &Adt::from(s).into())
                }
                VariantDef::Union(u) => {
                    get_container_name(sems, db, &Adt::from(u).into())
                }
            });
            container_names.push(name_to_string(parent.name(db)))
        }
        Definition::Local(l) => {
            let parent = l.parent(db);
            let parent_name = parent.name(db);
            let parent_def = match parent {
                DefWithBody::Function(f) => f.into(),
                DefWithBody::Static(s) => s.into(),
                DefWithBody::Const(c) => c.into(),
                DefWithBody::Variant(v) => v.into(),
                DefWithBody::InTypeConst(_) => unimplemented!("TODO"),
            };
            container_names.append(&mut get_container_name(sems, db, &parent_def));
            container_names.push(parent_name.map(name_to_string).unwrap_or_default())
        }
        Definition::Function(f) => {
            if let Some(item) = f.as_assoc_item(db) {
                match item.container(db) {
                    AssocItemContainer::Trait(t) => {
                        let mut parent_name = get_container_name(sems, db, &t.into());
                        container_names.append(&mut parent_name);
                        container_names.push(name_to_string(t.name(db)))
                    }
                    AssocItemContainer::Impl(i) => {
                        let adt = i.self_ty(db).as_adt();
                        let name = adt.map(|it| name_to_string(it.name(db)));
                        let mut parent_names = get_container_name(sems, db, &i.into());
                        container_names.append(&mut parent_names);
                        container_names.push(name.unwrap_or_default())
                    }
                }
            }
            // If the function is defined inside another function body,
            // get the name of the containing function
            else if let ModuleSource::BlockExpr(bl_expr) =
                f.module(db).definition_source(db).value
            {
                let str = bl_expr
                    .syntax()
                    .parent()
                    .and_then(|parent| {
                        ra_ap_syntax::ast::Fn::cast(parent).and_then(|function| {
                            let parent_def = sems.to_def(&function)?.into();
                            let mut name = get_container_name(sems, db, &parent_def);
                            container_names.append(&mut name);
                            Some(function.name()?.as_name())
                        })
                    })
                    .map(name_to_string)
                    .unwrap_or_default();
                container_names.push(str);
            }
        }
        Definition::Variant(e) => {
            container_names.push(name_to_string(e.parent_enum(db).name(db)))
        }
        _ => {
            // If the definition exists inside a function body,
            // get the name of the containing function
            if def.module(db).is_none() {
                container_names.push(String::new());
            } else if let ModuleSource::BlockExpr(bl_expr) =
                def.module(db).unwrap().definition_source(db).value
            {
                let str = bl_expr
                    .syntax()
                    .parent()
                    .and_then(|parent| {
                        ra_ap_syntax::ast::Fn::cast(parent).and_then(|function| {
                            let parent_def = sems.to_def(&function)?.into();
                            let mut name = get_container_name(sems, db, &parent_def);
                            container_names.append(&mut name);
                            Some(function.name()?.as_name())
                        })
                    })
                    .map(name_to_string)
                    .unwrap_or_default();
                container_names.push(str)
            }
        }
    }
    container_names.retain(|s| !s.is_empty());

    container_names
}

// Get the fully resolved type of a definition.
// e.g in pub fn test() -> Option<&(dyn Error + 'static)> { None }
// the type of function 'test' is core::option::Option<&dyn core::error::Error>
fn get_canonical_type(
    sems: &Semantics<RootDatabase>,
    db: &RootDatabase,
    def: &Definition,
) -> Result<CanonicalType> {
    let mut type_defs: Vec<Definition> = Vec::new();
    let mut trait_bounds: Vec<CanonicalPath> = Vec::new();
    let mut ty_kind = TypeKind::Plain;
    let mut type_;

    let mut push = |d| {
        if !type_defs.contains(&d) {
            type_defs.push(d);
        }
    };

    let mut resolve_bounds = |tr| {
        if let Some(b) = canonical_path(sems, db, &tr) {
            if !trait_bounds.contains(&b) {
                trait_bounds.push(b);
            }
        }
    };

    let ty = match def {
        Definition::Adt(it) => Some(it.ty(db)),
        Definition::Local(it) => Some(it.ty(db)),
        Definition::Const(it) => Some(it.ty(db)),
        Definition::SelfType(it) => Some(it.self_ty(db)),
        Definition::TypeAlias(it) => Some(it.ty(db)),
        Definition::BuiltinType(it) => Some(it.ty(db)),
        Definition::Function(it) => {
            ty_kind = TypeKind::Function;
            Some(it.ret_type(db))
        }
        Definition::Static(it) => {
            if it.is_mut(db) {
                ty_kind = TypeKind::StaticMut;
            }
            Some(it.ty(db))
        }
        Definition::Field(it) => {
            if let VariantDef::Union(_) = &it.parent_def(db) {
                ty_kind = TypeKind::UnionFld;
            }
            Some(it.ty(db))
        }
        Definition::GenericParam(GenericParam::TypeParam(it)) => {
            it.trait_bounds(db).into_iter().for_each(|it| resolve_bounds(it.into()));
            Some(it.ty(db))
        }
        Definition::GenericParam(GenericParam::ConstParam(it)) => Some(it.ty(db)),
        Definition::Variant(it) => {
            match canonical_path(sems, db, def) {
                Some(cp) => {
                    return Ok(CanonicalType::new_owned(
                        cp.as_str().to_string(),
                        vec![],
                        ty_kind,
                    ))
                }
                None => {
                    return Err(anyhow!(
                        "Could not resolve type for definition {:?}",
                        def.name(db)
                    ))
                }
            };
        }
        _ => None,
    };

    if ty.is_none()
    /*|| (ty.is_some() && ty.clone().unwrap().contains_unknown())*/
    {
        return Err(anyhow!("Could not resolve type for definition {:?}", def.name(db)));
    }

    let ty = ty.unwrap();
    if ty.impls_fnonce(db) {
        // impls_fnonce can be used in RA to check if a type is callable.
        // FnOnce is a supertait of FnMut and Fn, so any callable type
        // implements at least FnOnce.
        // TODO: More sophisticated checks are needed to precisely
        // determine which trait is actually implemented.
        ty_kind = TypeKind::Callable(crate::ident::CallableKind::FnOnce)
    }
    if ty.is_closure() {
        ty_kind = TypeKind::Callable(crate::ident::CallableKind::Closure)
    }
    if ty.is_fn() {
        ty_kind = TypeKind::Callable(crate::ident::CallableKind::FnPtr)
    }
    if ty.is_raw_ptr() {
        ty_kind = TypeKind::RawPointer
    }
    if ty.as_dyn_trait().is_some() {
        ty_kind = TypeKind::DynTrait
    }
    if ty.as_type_param(db).is_some() {
        ty_kind = TypeKind::Generic
    }

    // Get the type as it is displayed by rust-analyzer
    type_ = ty.display(db).to_string();

    // Walk type and gather all Enums, Structs, Unions,
    // or Traits it contains to fully resolve them
    ty.walk(db, |t| {
        if let Some(adt) = t.as_adt() {
            push(adt.into());
        } else if let Some(trait_) = t.as_dyn_trait() {
            push(trait_.into());
        } else if let Some(traits) = t.as_impl_traits(db) {
            traits.for_each(|it| push(it.into()));
        } else if let Some(trait_) = t.as_associated_type_parent_trait(db) {
            push(trait_.into());
        }
    });

    // Resolve types.
    // e.g 'Option' resolves to 'core::option::Option'.
    type_defs.into_iter().for_each(|it| {
        type_ = canonical_path(sems, db, &it).map_or(type_.clone(), |cp| {
            type_.replace(
                name_to_string(it.name(db).expect("Definition should a have name"))
                    .as_str(),
                cp.as_str(),
            )
        })
    });

    Ok(CanonicalType::new_owned(type_, trait_bounds, ty_kind))
}

//Note: This is still under development
fn _is_unsafe_callable_ty(def: Definition, db: &RootDatabase) -> Result<bool> {
    match def {
        Definition::Const(c) => {
            let data = db.const_data(c.into());
            if let TypeRef::Fn(_, _, is_unsafe) = data.type_ref.as_ref() {
                return Ok(*is_unsafe);
            }
        }
        Definition::Static(s) => {
            let data = db.static_data(s.into());
            if let TypeRef::Fn(_, _, is_unsafe) = data.type_ref.as_ref() {
                return Ok(*is_unsafe);
            }
        }
        Definition::TypeAlias(alias) => {
            let id = TypeAliasId::from(alias);
            let ty = db.ty(id.into());
            let data = db.type_alias_data(id);
            let kind = ty.skip_binders().kind(Interner);

            if let TyKind::Function(fn_ptr) = kind {
                return Ok(matches!(fn_ptr.sig.safety, ra_ap_hir_ty::Safety::Unsafe));
            } else if let TyKind::Adt(
                ra_ap_hir_ty::AdtId(ra_ap_hir_def::AdtId::StructId(_)),
                substs,
            ) = kind
            {
                return _is_unsafe_callable_in_smart_ptr(substs);
            }
        }
        Definition::Local(l) => {
            let def = l.parent(db);
            let infer = db.infer(def.into());

            return infer
                .type_of_pat
                .iter()
                // TODO: modified:
                .find(|(pat_id, ty)| {
                    // TODO: how to convert pat_id to BindingId?
                    // Is the below fix correct?
                    let b = ra_ap_hir_def::hir::BindingId::from_raw(pat_id.into_raw());
                    l == ra_ap_hir::Local::from((def.into(), b))
                })
                .map(|(_, ty)| {
                    let kind = ty.kind(Interner);
                    match kind {
                        TyKind::Function(fn_pointer) => Ok(matches!(
                            fn_pointer.sig.safety,
                            ra_ap_hir_ty::Safety::Unsafe
                        )),
                        TyKind::Adt(
                            ra_ap_hir_ty::AdtId(ra_ap_hir_def::AdtId::StructId(_)),
                            substs,
                        ) => _is_unsafe_callable_in_smart_ptr(substs),
                        _ => Ok(false),
                    }
                })
                .unwrap_or_else(|| Ok(false));
        }
        Definition::Field(fld) => {
            let var_id: ra_ap_hir_def::VariantId = fld.parent_def(db).into();
            let fld_id = ra_ap_hir_def::FieldId::from(fld);

            let generic_def_id = ra_ap_hir_def::GenericDefId::from(var_id.adt_id());
            let substs = ra_ap_hir_ty::TyBuilder::placeholder_subst(db, generic_def_id);
            let ty = db.field_types(var_id)[fld_id.local_id]
                .clone()
                .substitute(Interner, &substs);
            let kind = ty.kind(Interner);

            if let TyKind::Function(fn_pointer) = kind {
                return Ok(matches!(fn_pointer.sig.safety, ra_ap_hir_ty::Safety::Unsafe));
            } else if let TyKind::Adt(
                ra_ap_hir_ty::AdtId(ra_ap_hir_def::AdtId::StructId(_)),
                substs,
            ) = kind
            {
                return _is_unsafe_callable_in_smart_ptr(substs);
            }
        }
        _ => (),
    }

    Ok(false)
}

// This case is for callable items inside smart pointers, like 'Box<unsafe fn(i32, i32) -> i32>'
fn _is_unsafe_callable_in_smart_ptr(substs: &ra_ap_hir_ty::Substitution) -> Result<bool> {
    substs
        .iter(Interner)
        .find(|subst| match subst.ty(Interner) {
            Some(ty) => matches!(ty.kind(Interner), TyKind::Function(_)),
            None => false,
        })
        .map(|subst| {
            if let TyKind::Function(fn_pointer) =
                subst.ty(Interner).unwrap().kind(Interner)
            {
                matches!(fn_pointer.sig.safety, ra_ap_hir_ty::Safety::Unsafe)
            } else {
                false
            }
        })
        .ok_or_else(|| {
            anyhow!(
                "Could not determine if it is an unsafe callable inside a smart pointer"
            )
        })
}
