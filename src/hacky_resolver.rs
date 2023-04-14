//! A hacky in-house resolver for Rust identifiers

use super::ident::{CanonicalPath, Ident, Path};
use super::resolve::Resolve;

use anyhow::Result;
use std::collections::HashMap;
use std::fmt::Debug;
use std::path::Path as FilePath;
use syn::{self, spanned::Spanned};

// Incorrectly infer mods from filepath
mod infer {
    use std::path::Path;

    fn infer_crate(filepath: &Path) -> String {
        let crate_src: Vec<String> = filepath
            .iter()
            .map(|x| {
                x.to_str().unwrap_or_else(|| {
                    panic!("found path that wasn't a valid UTF-8 string: {:?}", x)
                })
            })
            .take_while(|&x| x != "src")
            .map(|x| x.to_string())
            .collect();
        let crate_string = crate_src.last().cloned().unwrap_or_else(|| {
            eprintln!("warning: unable to infer crate from path: {:?}", filepath);
            "".to_string()
        });
        crate_string
    }

    fn infer_module(filepath: &Path) -> Vec<String> {
        let post_src: Vec<String> = filepath
            .iter()
            .map(|x| {
                x.to_str().unwrap_or_else(|| {
                    panic!("found path that wasn't a valid UTF-8 string: {:?}", x)
                })
            })
            .skip_while(|&x| x != "src")
            .skip(1)
            .filter(|&x| x != "main.rs" && x != "lib.rs")
            .map(|x| x.replace(".rs", ""))
            .collect();
        post_src
    }

    pub fn fully_qualified_prefix(filepath: &Path) -> String {
        let mut prefix_vec = vec![infer_crate(filepath)];
        let mut mod_vec = infer_module(filepath);
        prefix_vec.append(&mut mod_vec);
        prefix_vec.join("::")
    }
}

#[derive(Debug)]
pub struct HackyResolver<'a> {
    // crate+module which the current filepath implements (e.g. my_crate::fs)
    modpath: CanonicalPath,

    // stack-based scope
    scope_use: Vec<&'a syn::Ident>,
    scope_mods: Vec<&'a syn::Ident>,
    scope_fun: Vec<&'a syn::Ident>,
    scope_fun_lens: Vec<usize>,
    scope_impl_adds: Vec<usize>,

    // use name lookups
    use_names: HashMap<&'a syn::Ident, Vec<&'a syn::Ident>>,
    ffi_decls: HashMap<&'a syn::Ident, CanonicalPath>,

    // TBD: unused
    use_globs: Vec<Vec<&'a syn::Ident>>,
}

impl<'a> Resolve<'a> for HackyResolver<'a> {
    fn assert_top_level_invariant(&self) {
        debug_assert!(self.scope_use.is_empty());
        debug_assert!(self.scope_mods.is_empty());
        debug_assert!(self.scope_fun.is_empty());
        debug_assert!(self.scope_fun_lens.is_empty());
        debug_assert!(self.scope_impl_adds.is_empty());
    }

    fn push_mod(&mut self, mod_ident: &'a syn::Ident) {
        // modules can exist inside of functions apparently
        // handling that in what seems to be the most straightforward way
        self.scope_fun_lens.push(self.scope_fun.len());
        self.scope_mods.append(&mut self.scope_fun);
        debug_assert!(self.scope_fun.is_empty());
        self.scope_mods.push(mod_ident);
    }

    fn pop_mod(&mut self) {
        self.scope_mods.pop();
        // restore scope_fun state
        let n = self.scope_fun_lens.pop().unwrap();
        for _ in 0..n {
            self.scope_fun.push(self.scope_mods.pop().unwrap());
        }
    }

    fn push_impl(&mut self, impl_stmt: &'a syn::ItemImpl) {
        if let Some((_, tr, _)) = &impl_stmt.trait_ {
            // scope trait impls under trait name
            let scope_adds = self.scan_impl_trait_path(tr);
            self.scope_impl_adds.push(scope_adds);
        } else {
            // scope type impls under type name
            let scope_adds = self.scan_impl_type(&impl_stmt.self_ty);
            self.scope_impl_adds.push(scope_adds);
        };
    }

    fn pop_impl(&mut self) {
        let scope_adds = self.scope_impl_adds.pop().unwrap();
        for _ in 0..scope_adds {
            self.scope_mods.pop();
        }
    }

    fn push_fn(&mut self, fn_ident: &'a syn::Ident) {
        self.scope_fun.push(fn_ident);
    }

    fn pop_fn(&mut self) {
        self.scope_fun.pop();
    }

    fn scan_use(&mut self, use_path: &'a syn::ItemUse) {
        // TBD: may need to do something special here if already inside a fn
        // (scope_fun is nonempty)
        self.scan_use_tree(&use_path.tree);
    }

    fn scan_foreign_fn(&mut self, f: &'a syn::ForeignItemFn) {
        let fn_name = &f.sig.ident;
        let fn_path = self.resolve_def(fn_name);
        self.ffi_decls.insert(fn_name, fn_path);
    }

    fn resolve_ident(&self, i: &'a syn::Ident) -> CanonicalPath {
        Self::aggregate_path(self.lookup_ident_vec(&i))
    }

    fn resolve_path(&self, p: &'a syn::Path) -> CanonicalPath {
        Self::aggregate_path(&self.lookup_path_vec(p))
    }

    fn resolve_def(&self, i: &'a syn::Ident) -> CanonicalPath {
        let mut result = self.modpath.clone();

        // Push current mod scope [ "mod1", "mod2", ...]
        result.append_path(&self.get_mod_scope());

        // Push definition ident
        result.push_ident(&Ident::from_syn(i));

        result
    }

    fn resolve_ffi(&self, ffi: &syn::Path) -> Option<CanonicalPath> {
        // TBD lookup
        let span = &ffi.segments.last().unwrap().ident;

        self.ffi_decls.get(span).cloned()
    }
}

impl<'a> HackyResolver<'a> {
    pub fn new(filepath: &FilePath) -> Result<Self> {
        // TBD: incomplete, replace with name resolution
        let modpath = CanonicalPath::new_owned(infer::fully_qualified_prefix(filepath));

        Ok(Self {
            modpath,
            scope_use: Vec::new(),
            scope_mods: Vec::new(),
            scope_fun: Vec::new(),
            scope_fun_lens: Vec::new(),
            scope_impl_adds: Vec::new(),
            use_names: HashMap::new(),
            ffi_decls: HashMap::new(),
            use_globs: Vec::new(),
        })
    }

    /// Reusable warning logger
    fn syn_warning<S: Spanned + Debug>(&self, msg: &str, syn_node: S) {
        let line = syn_node.span().start().line;
        let col = syn_node.span().start().column;
        eprintln!("Warning (Resolver): {} ({:?}) ({}:{})", msg, syn_node, line, col);
    }

    /*
        Use statements
    */

    fn scope_use_snapshot(&self) -> Vec<&'a syn::Ident> {
        self.scope_use.clone()
    }

    fn save_scope_use_under(&mut self, lookup_key: &'a syn::Ident) {
        // save the use scope under an identifier/lookup key
        let v_new = self.scope_use_snapshot();
        if cfg!(debug) && self.use_names.contains_key(lookup_key) {
            let v_old = self.use_names.get(lookup_key).unwrap();
            if *v_old != v_new {
                eprintln!(
                    "Name conflict found in use scope: {:?} (old: {:?} new: {:?})",
                    lookup_key, v_old, v_new
                );
            }
        }
        self.use_names.insert(lookup_key, v_new);
    }

    fn scan_use_tree(&mut self, u: &'a syn::UseTree) {
        match u {
            syn::UseTree::Path(p) => self.scan_use_path(p),
            syn::UseTree::Name(n) => self.scan_use_name(n),
            syn::UseTree::Rename(r) => self.scan_use_rename(r),
            syn::UseTree::Glob(g) => self.scan_use_glob(g),
            syn::UseTree::Group(g) => self.scan_use_group(g),
        }
    }

    fn scan_use_path(&mut self, p: &'a syn::UsePath) {
        self.scope_use.push(&p.ident);
        self.scan_use_tree(&p.tree);
        self.scope_use.pop();
    }

    fn scan_use_name(&mut self, n: &'a syn::UseName) {
        self.scope_use.push(&n.ident);
        self.save_scope_use_under(&n.ident);
        self.scope_use.pop();
    }

    fn scan_use_rename(&mut self, r: &'a syn::UseRename) {
        self.scope_use.push(&r.ident);
        self.save_scope_use_under(&r.rename);
        self.scope_use.pop();
    }

    fn scan_use_glob(&mut self, _g: &'a syn::UseGlob) {
        self.use_globs.push(self.scope_use_snapshot());
    }

    fn scan_use_group(&mut self, g: &'a syn::UseGroup) {
        for t in g.items.iter() {
            self.scan_use_tree(t);
        }
    }

    /*
        Impl blocks
    */
    fn scan_impl_type(&mut self, ty: &'a syn::Type) -> usize {
        // return: the number of items added to scope_mods
        match ty {
            syn::Type::Group(x) => self.scan_impl_type(&x.elem),
            syn::Type::Paren(x) => self.scan_impl_type(&x.elem),
            syn::Type::Path(x) => self.scan_impl_type_path(&x.path),
            syn::Type::TraitObject(x) => {
                self.syn_warning("skipping 'impl dyn Trait' block", x);
                0
            }
            syn::Type::Verbatim(v) => {
                self.syn_warning("skipping Verbatim expression", v);
                0
            }
            // TBD
            // syn::Type::Macro(_) => {
            //     self.data.skipped_macros += 1;
            //     0
            // }
            _ => {
                self.syn_warning("unexpected impl block type (ignoring)", ty);
                0
            }
        }
        // other cases -- mostly built-ins, so shouldn't really occur in
        // impl blocks; only in impl Trait for blocks, which we should handle
        // separately
        // Array(x) => {}
        // BareFn(x) => {}
        // ImplTrait(x) => {}
        // Infer(x) => {}
        // Never(x) => {}
        // Ptr(x) => {}
        // Reference(x) => {}
        // Slice(x) => {}
        // TraitObject(x) => {}
        // Tuple(x) => {}
    }
    fn scan_impl_type_path(&mut self, ty: &'a syn::Path) -> usize {
        // return: the number of items added to scope_mods
        let fullpath = self.lookup_path_vec(ty);
        self.scope_mods.extend(&fullpath);
        if fullpath.is_empty() {
            self.syn_warning("unexpected empty impl type path", ty);
        }
        fullpath.len()
    }
    fn scan_impl_trait_path(&mut self, tr: &'a syn::Path) -> usize {
        // return: the number of items added to scope_mods
        let fullpath = self.lookup_path_vec(tr);
        self.scope_mods.extend(&fullpath);
        if fullpath.is_empty() {
            self.syn_warning("unexpected empty trait name path", tr);
        }
        fullpath.len()
    }

    /*
        Name resolution methods
    */

    // weird signature: need a double reference on i because i is owned by cur function
    // all hail the borrow checker for catching this error
    fn lookup_ident_vec<'c>(&'c self, i: &'c &'a syn::Ident) -> &'c [&'a syn::Ident]
    where
        'a: 'c,
    {
        self.use_names
            .get(i)
            .map(|v| v.as_slice())
            .unwrap_or_else(|| std::slice::from_ref(i))
    }

    // this one creates a new path, so it has to return a Vec anyway
    // precond: input path must be nonempty
    // return: nonempty Vec of identifiers in the full path
    fn lookup_path_vec(&self, p: &'a syn::Path) -> Vec<&'a syn::Ident> {
        let mut result = Vec::new();
        let mut it = p.segments.iter().map(|seg| &seg.ident);

        // first part of the path based on lookup
        let fst: &'a syn::Ident = it.next().unwrap();
        result.extend(self.lookup_ident_vec(&fst));
        // second part of the path based on any additional sub-scoping
        result.extend(it);

        result
    }

    fn get_mod_scope(&self) -> Path {
        Path::from_idents(self.scope_mods.iter().cloned().map(Ident::from_syn))
    }

    fn aggregate_path(p: &[&'a syn::Ident]) -> CanonicalPath {
        let mut result = Path::new_empty();
        for &i in p {
            result.push_ident(&Ident::from_syn(i));
        }
        CanonicalPath::from_path(result)
    }
}
