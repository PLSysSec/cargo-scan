use anyhow::{anyhow, Result};
use itertools::Itertools;
use ra_ap_hir_expand::InFile;

use crate::ident::{CanonicalPath, CanonicalType, Ident, TypeKind};

use ra_ap_hir::{
    Adt, AsAssocItem, AssocItemContainer, DefWithBody, GenericParam, HasSource,
    HirDisplay, Module, ModuleSource, Semantics, VariantDef,
};

use ra_ap_hir_expand::name::AsName;
use ra_ap_ide::{RootDatabase, TextSize};
use ra_ap_ide_db::base_db::SourceDatabase;
use ra_ap_ide_db::defs::Definition;

use ra_ap_syntax::ast::HasName;
use ra_ap_syntax::{AstNode, SourceFile, SyntaxNode, SyntaxToken, TokenAtOffset};

// latest rust-analyzer has removed Display for Name, see
// https://docs.rs/ra_ap_hir/latest/ra_ap_hir/struct.Name.html#
// This is a wrapper function to recover the .to_string() implementation
fn name_to_string(n: ra_ap_hir::Name) -> String {
    n.to_smol_str().to_string()
}

pub(super) fn get_token(
    src_file: &SourceFile,
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

pub(super) fn canonical_path(
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

/// Helper function to construct the canonical path
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
pub(super) fn get_canonical_type(
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
        Definition::Variant(_) => {
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

/// Get source node from the original source  
/// file for the given definition.
pub(super) fn syntax_node_from_def(
    def: &Definition,
    db: &RootDatabase,
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
        Definition::ExternCrateDecl(x) => x.source(db)?.syntax().original_syntax_node(db),
        _ => None,
    }
}
