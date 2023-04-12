//! A hacky in-house resolver for Rust identifiers

use super::ident::{CanonicalPath, Ident, Path};
use super::resolve::Resolve;

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path as FilePath;
use syn;

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
    // crate+module which the current filepath implements (e.g. mycrate::fs)
    modpath: CanonicalPath,

    // stack-based scope
    scope: usize,
    scope_use: Vec<&'a syn::Ident>,

    // use name lookups
    use_names: HashMap<&'a syn::Ident, Vec<&'a syn::Ident>>,
    use_globs: Vec<Vec<&'a syn::Ident>>,
}

impl<'a> Resolve<'a> for HackyResolver<'a> {
    fn new(filepath: &FilePath) -> Result<Self> {
        // TBD: incomplete, replace with name resolution
        let modpath = CanonicalPath::new_owned(infer::fully_qualified_prefix(filepath));

        Ok(Self {
            modpath,
            scope: 0,
            scope_use: Vec::new(),
            use_names: HashMap::new(),
            use_globs: Vec::new(),
        })
    }

    fn assert_invariant(&self) {
        if self.scope == 0 {
            debug_assert!(self.scope_use.is_empty());
        }
    }

    fn push_scope(&mut self) {
        self.scope += 1;
    }

    fn pop_scope(&mut self) {
        self.scope = self.scope.saturating_sub(1);
    }

    fn scan_use(&mut self, use_path: &'a syn::ItemUse) {
        // TBD: may need to do something special here if already inside a fn
        // (scope_fun is nonempty)
        self.scan_use_tree(&use_path.tree);
    }

    fn resolve_ident(&self, i: &'a syn::Ident) -> Path {
        Self::aggregate_path(self.lookup_ident_vec(&i))
    }

    fn resolve_path(&self, p: &'a syn::Path) -> Path {
        Self::aggregate_path(&self.lookup_path_vec(p))
    }

    fn resolve_ident_canonical(&self, i: &'a syn::Ident) -> CanonicalPath {
        let mut result = self.modpath.clone();
        result.append_path(&self.resolve_ident(i));
        result
    }

    fn resolve_path_canonical(&self, _i: &'a syn::Path) -> CanonicalPath {
        todo!()
    }

    fn get_modpath(&self) -> CanonicalPath {
        self.modpath.clone()
    }

    fn lookup_path_vec(&self, p: &'a syn::Path) -> Vec<&'a syn::Ident> {
        self.lookup_path_vec_core(p)
    }
}

impl<'a> HackyResolver<'a> {
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
        Name resolution methods
    */

    // weird signature: need a double reference on i because i is owned by cur function
    // all hail the borrow checker for catching this error
    pub fn lookup_ident_vec<'c>(&'c self, i: &'c &'a syn::Ident) -> &'c [&'a syn::Ident]
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
    pub fn lookup_path_vec_core(&self, p: &'a syn::Path) -> Vec<&'a syn::Ident> {
        let mut result = Vec::new();
        let mut it = p.segments.iter().map(|seg| &seg.ident);

        // first part of the path based on lookup
        let fst: &'a syn::Ident = it.next().unwrap();
        result.extend(self.lookup_ident_vec(&fst));
        // second part of the path based on any additional sub-scoping
        result.extend(it);

        result
    }

    fn syn_to_ident(i: &syn::Ident) -> Ident {
        Ident::new_owned(i.to_string())
    }

    fn aggregate_path(p: &[&'a syn::Ident]) -> Path {
        let mut result = Path::new_empty();
        for &i in p {
            result.push_ident(&Self::syn_to_ident(i));
        }
        result
    }
}
