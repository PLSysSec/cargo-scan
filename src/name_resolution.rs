#![allow(unused_variables)]

use std::fs::canonicalize;
use std::path::Path;

use anyhow::{anyhow, Result};
use log::debug;
use ra_ap_hir_def::db::DefDatabase;
use ra_ap_hir_def::{FunctionId, Lookup};
use ra_ap_hir_expand::name::AsName;
use ra_ap_ide_db::base_db::{SourceDatabase, Upcast};
use ra_ap_ide_db::FxHashMap;
use ra_ap_syntax::ast::HasName;

use crate::ident::TypeKind;

use super::effect::SrcLoc;
use super::ident::{CanonicalPath, CanonicalType, Ident};

use ra_ap_hir::{
    Adt, AsAssocItem, AssocItemContainer, DefWithBody, GenericParam, HirDisplay, Module,
    ModuleSource, Semantics, VariantDef,
};
use ra_ap_ide::{AnalysisHost, FileId, LineCol, RootDatabase, TextSize};
use ra_ap_ide_db::defs::{Definition, IdentClass};
use ra_ap_paths::AbsPathBuf;
use ra_ap_project_model::{
    CargoConfig, CargoFeatures, InvocationLocation, InvocationStrategy, ProjectManifest,
    ProjectWorkspace, RustcSource, UnsetTestCrates,
};

use ra_ap_rust_analyzer::cli::load_cargo::{load_workspace, LoadCargoConfig};
use ra_ap_syntax::{AstNode, SourceFile, SyntaxToken, TokenAtOffset};
use ra_ap_vfs::{Vfs, VfsPath};

use itertools::Itertools;

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
        let sysroot = Some(RustcSource::Discover);

        // rustc private crate source
        let rustc_source = None;

        // crates to disable `#[cfg(test)]` on
        let unset_test_crates = UnsetTestCrates::Only(vec![String::from("core")]);

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
        debug!("Creating resolver with path {:?}", crate_path);

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

        debug!("...created");

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

    fn token(
        &self,
        sems: &Semantics<RootDatabase>,
        s: SrcLoc,
        i: Ident,
    ) -> Result<SyntaxToken> {
        let mut filepath = s.dir().clone();
        filepath.push(s.file().as_path());
        let file_id = self.get_file_id(&filepath, sems)?;
        let offset = self.find_offset(file_id, s)?;
        let src_file = sems.parse(file_id);

        get_token(src_file, offset, i)
    }

    pub fn resolve_ident(&self, s: SrcLoc, i: Ident) -> Result<CanonicalPath> {
        let db = self.get_db();
        let sems = self.get_semantics();
        let token = self.token(&sems, s, i)?;
        let def = find_def(token, &sems, db)?;
        canonical_path(&sems, db, &def)
            .ok_or_else(|| anyhow!("Could not construct canonical path for '{:?}'", def))
    }

    pub fn resolve_type(&self, s: SrcLoc, i: Ident) -> Result<CanonicalType> {
        let db = self.get_db();
        let sems = self.get_semantics();
        let token = self.token(&sems, s, i)?;
        debug!("token: {:?}", token);
        let def = find_def(token, &sems, db)?;
        get_canonical_type(&sems, db, &def)
    }

    pub fn is_ffi(&self, s: SrcLoc, i: Ident) -> Result<bool> {
        let db = self.get_db();
        let sems = self.get_semantics();
        let token = self.token(&sems, s, i)?;
        let def = find_def(token, &sems, db)?;

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
            _ => Ok(false),
        }
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
    token: SyntaxToken,
    sems: &Semantics<RootDatabase>,
    db: &RootDatabase,
) -> Result<Definition> {
    sems.descend_into_macros(token.clone())
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
        .or(Err(anyhow!("Could not classify token {:?}", token.to_string())))
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
        return Some(CanonicalPath::new_owned(b.name().to_string()));
    }

    let container = get_container_name(sems, db, def);
    let def_name = def.name(db).map(|name| name.to_string());
    let module = def.module(db)?;

    let crate_name = db.crate_graph()[module.krate().into()]
        .display_name
        .as_ref()
        .map(|it| it.to_string());
    let module_path = build_path_to_root(module, db)
        .into_iter()
        .rev()
        .flat_map(|it| it.name(db).map(|name| name.to_string()));

    let cp = crate_name
        .into_iter()
        .chain(module_path)
        .chain(container)
        .chain(def_name)
        .join("::");

    Some(CanonicalPath::new_owned(cp))
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
            container_names.push(parent.name(db).to_string())
        }
        Definition::Local(l) => {
            let parent = l.parent(db);
            let parent_name = parent.name(db);
            let parent_def = match parent {
                DefWithBody::Function(f) => f.into(),
                DefWithBody::Static(s) => s.into(),
                DefWithBody::Const(c) => c.into(),
                DefWithBody::Variant(v) => v.into(),
            };
            container_names.append(&mut get_container_name(sems, db, &parent_def));
            container_names.push(parent_name.map(|n| n.to_string()).unwrap_or_default())
        }
        Definition::Function(f) => {
            if let Some(item) = f.as_assoc_item(db) {
                match item.container(db) {
                    AssocItemContainer::Trait(t) => {
                        let mut parent_name = get_container_name(sems, db, &t.into());
                        container_names.append(&mut parent_name);
                        container_names.push(t.name(db).to_string())
                    }
                    AssocItemContainer::Impl(i) => {
                        let adt = i.self_ty(db).as_adt();
                        let name = adt.map(|it| it.name(db).to_string());
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
                    .map(|name| name.to_string())
                    .unwrap_or_default();
                container_names.push(str);
            }
        }
        Definition::Variant(e) => {
            container_names.push(e.parent_enum(db).name(db).to_string())
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
                    .map(|name| name.to_string())
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
        Definition::Field(it) => Some(it.ty(db)),
        Definition::Const(it) => Some(it.ty(db)),
        Definition::Static(it) => Some(it.ty(db)),
        Definition::SelfType(it) => Some(it.self_ty(db)),
        Definition::TypeAlias(it) => Some(it.ty(db)),
        Definition::BuiltinType(it) => Some(it.ty(db)),
        Definition::Function(it) => Some(it.ret_type(db)),
        Definition::GenericParam(GenericParam::TypeParam(it)) => {
            it.trait_bounds(db).into_iter().for_each(|it| resolve_bounds(it.into()));
            Some(it.ty(db))
        }
        Definition::GenericParam(GenericParam::ConstParam(it)) => Some(it.ty(db)),
        Definition::Variant(it) => it
            .value(db)
            .and_then(|expr| sems.type_of_expr(&expr).map(|info| info.original())),
        _ => None,
    };

    if ty.is_none() || (ty.is_some() && ty.clone().unwrap().contains_unknown()) {
        return Err(anyhow!("Could not resolve type for definition {:?}", def));
    }
    let ty = ty.unwrap();
    let ty_kind = if ty.is_closure() {
        TypeKind::Callable(crate::ident::CallableKind::Closure)
    } else if ty.is_fn() {
        TypeKind::Callable(crate::ident::CallableKind::FnPtr)
    } else if ty.impls_fnonce(db) {
        // impls_fnonce can be used to check if a type is callable.
        // FnOnce is a supertait of FnMut and Fn, so any callable type
        // implements at least FnOnce.
        // TODO: More sophisticated checks are needed to precisely
        // determine which trait is actually implemented.
        TypeKind::Callable(crate::ident::CallableKind::FnOnce)
    } else if ty.is_raw_ptr() {
        TypeKind::RawPointer
    } else if ty.as_dyn_trait().is_some() {
        TypeKind::DynTrait
    } else if ty.as_type_param(db).is_some() {
        TypeKind::Generic
    } else {
        TypeKind::Plain
    };

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
                it.name(db).expect("Definition should a have name").to_string().as_str(),
                cp.as_str(),
            )
        })
    });

    Ok(CanonicalType::new_owned(type_, trait_bounds, ty_kind))
}
