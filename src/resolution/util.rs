use anyhow::{anyhow, Result};
use itertools::Itertools;

use crate::ident::{CanonicalPath, CanonicalType, Ident, TypeKind};

use ra_ap_hir::{
    Adt, DefWithBody, DisplayTarget, ExpressionStoreOwner, GenericParam, HasContainer,
    HasCrate, HirDisplay, Module, Semantics, Variant,
};

use ra_ap_ide::TextSize;
use ra_ap_ide_db::{base_db::all_crates, defs::Definition, RootDatabase};

use ra_ap_syntax::{SyntaxNode, SyntaxToken, TokenAtOffset};

// latest rust-analyzer has removed Display for Name, see
// https://docs.rs/ra_ap_hir/latest/ra_ap_hir/struct.Name.html#
// This is a wrapper function to recover the .to_string() implementation
fn name_to_string(n: ra_ap_hir::Name) -> String {
    n.as_str().to_string()
}

pub(super) fn get_token(
    src_file: &SyntaxNode,
    offset: TextSize,
    ident: Ident,
) -> Result<SyntaxToken> {
    match src_file.token_at_offset(offset) {
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
    _sems: &Semantics<RootDatabase>,
    db: &RootDatabase,
    def: &Definition,
) -> Option<CanonicalPath> {
    if let Definition::BuiltinType(b) = def {
        return Some(CanonicalPath::new(&name_to_string(b.name())));
    }

    let container = get_container_name(db, def);
    let def_name = def.name(db).map(name_to_string);
    let module = def.module(db)?;

    let crates = all_crates(db);
    let crate_name = crates
        .iter()
        .filter(|cr| **cr == module.krate(db).base())
        .filter_map(|cr| cr.extra_data(db).display_name.as_ref())
        .map(|n| n.to_string());

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
fn get_container_name(db: &RootDatabase, def: &Definition) -> Vec<String> {
    let mut container_names = vec![];
    match def {
        Definition::Field(f) => {
            let parent = f.parent_def(db);
            container_names.append(&mut match parent {
                Variant::EnumVariant(v) => get_container_name(db, &v.into()),
                Variant::Struct(s) => get_container_name(db, &Adt::from(s).into()),
                Variant::Union(u) => get_container_name(db, &Adt::from(u).into()),
            });
            container_names.push(name_to_string(parent.name(db)))
        }
        Definition::Local(l) => {
            let (parent_def, parent_name) = match l.parent(db) {
                ExpressionStoreOwner::Body(DefWithBody::Function(f)) => {
                    (f.into(), Some(f.name(db)))
                }
                ExpressionStoreOwner::Body(DefWithBody::Static(s)) => {
                    (s.into(), Some(s.name(db)))
                }
                ExpressionStoreOwner::Body(DefWithBody::Const(c)) => {
                    (c.into(), c.name(db))
                }
                ExpressionStoreOwner::Body(DefWithBody::EnumVariant(v)) => {
                    (v.into(), Some(v.name(db)))
                }
                _ => {
                    container_names.retain(|s| !s.is_empty());
                    return container_names;
                }
            };
            container_names.append(&mut get_container_name(db, &parent_def));
            container_names.push(parent_name.map(name_to_string).unwrap_or_default())
        }
        Definition::Function(f) => match f.container(db) {
            ra_ap_hir::ItemContainer::Trait(t) => {
                container_names.append(&mut get_container_name(db, &t.into()));
                container_names.push(name_to_string(t.name(db)))
            }
            ra_ap_hir::ItemContainer::Impl(i) => {
                let krate = i.module(db).krate(db);
                let edition = krate.edition(db);
                let name = if let Some(trait_) = i.trait_(db) {
                    format!(
                        "<{} as {}>",
                        i.self_ty(db)
                            .display(db, DisplayTarget::from_crate(db, krate.into())),
                        trait_.name(db).display(db, edition)
                    )
                } else {
                    let adt = i.self_ty(db).as_adt();
                    adt.map(|it| name_to_string(it.name(db))).unwrap_or_default()
                };

                container_names.append(&mut get_container_name(db, &i.into()));
                container_names.push(name);
            }
            _ => {}
        },
        Definition::EnumVariant(e) => {
            container_names.push(name_to_string(e.parent_enum(db).name(db)))
        }
        Definition::Macro(m) => {
            container_names.append(&mut get_container_name(db, &m.module(db).into()));
            container_names
                .push(m.name(db).display(db, m.krate(db).edition(db)).to_string());
        }
        _ => {}
    }
    container_names.retain(|s| !s.is_empty());

    container_names
}

/// Type resolution
pub(super) fn get_canonical_type(
    db: &RootDatabase,
    def: &Definition,
) -> Result<CanonicalType> {
    let mut ty_kind = TypeKind::Plain;

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
            if let Variant::Union(_) = &it.parent_def(db) {
                ty_kind = TypeKind::UnionFld;
            }
            Some(it.ty(db))
        }
        Definition::GenericParam(GenericParam::TypeParam(it)) => Some(it.ty(db)),
        Definition::GenericParam(GenericParam::ConstParam(it)) => Some(it.ty(db)),
        Definition::EnumVariant(_) => return Ok(CanonicalType::new(ty_kind)),
        _ => None,
    }
    .ok_or_else(|| anyhow!("Could not resolve type for definition {:?}", def.name(db)))?;

    if ty.is_raw_ptr() {
        ty_kind = TypeKind::RawPointer
    }

    Ok(CanonicalType::new(ty_kind))
}
