/*
    Representation of an Effect location in source code
*/

use std::path::{Path as FilePath, PathBuf as FilePathBuf};

use super::ident::{Ident, Path, Pattern};

use serde::{Deserialize, Serialize};
use syn::spanned::Spanned;

fn sanitize_comma(s: &str) -> String {
    if s.contains(',') {
        eprintln!("Warning: ignoring unexpected comma when generating CSV: {s}");
    }
    s.replace(',', "")
}
fn sanitize_path(p: &FilePath) -> String {
    match p.to_str() {
        Some(s) => sanitize_comma(s),
        None => {
            eprintln!("Warning: path is invalid unicode: {:?}", p);
            sanitize_comma(&p.to_string_lossy())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectPathLoc {
    // Name of crate, e.g. num_cpus
    crt: Ident,
    // Full path to caller function, e.g. num_cpus::linux::logical_cpus
    caller: Path,
}
impl EffectPathLoc {
    pub fn new(filepath: &FilePath, mod_scope: &[String], caller: String) -> Self {
        // note: tries to infer the crate name and modules from the filepath
        // TBD: use Cargo.toml to get crate name & other info

        // Infer crate
        let pre_src: Vec<String> = filepath
            .iter()
            .map(|x| {
                x.to_str().unwrap_or_else(|| {
                    panic!("found path that wasn't a valid UTF-8 string: {:?}", x)
                })
            })
            .take_while(|&x| x != "src")
            .map(|x| x.to_string())
            .collect();
        let crt_str = pre_src.last().cloned().unwrap_or_else(|| {
            eprintln!("warning: unable to infer crate from path: {:?}", filepath);
            "".to_string()
        });
        let crt = Ident::new(&crt_str);

        // Infer module
        let mut post_src: Vec<String> = filepath
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

        // combine crate, module scope, and file-level modules (mod_scope)
        // to form full scope to caller
        let mut full_scope: Vec<String> = vec![crt_str];
        full_scope.append(&mut post_src);
        full_scope.extend_from_slice(mod_scope);
        full_scope.push(caller);

        let caller = Path::new_owned(full_scope.join("::"));
        Self { crt, caller }
    }
    pub fn csv_header() -> &'static str {
        "crate, caller"
    }
    pub fn to_csv(&self) -> String {
        let crt = sanitize_comma(self.crt.as_str());
        let caller = sanitize_comma(self.caller.as_str());
        format!("{}, {}", crt, caller)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SrcLoc {
    // Directory in which the call occurs
    dir: FilePathBuf,
    // File in which the call occurs -- in the above directory
    file: FilePathBuf,
    // Location in which the call occurs -- in the above file
    line: usize,
    col: usize,
}

impl SrcLoc {
    pub fn new(filepath: &FilePath, line: usize, col: usize) -> Self {
        // TBD: use unwrap_or_else
        let dir = filepath.parent().unwrap().to_owned();
        let file = FilePathBuf::from(filepath.file_name().unwrap());
        Self { dir, file, line, col }
    }
    pub fn csv_header() -> &'static str {
        "dir, file, line, col"
    }
    pub fn to_csv(&self) -> String {
        let dir = sanitize_path(&self.dir);
        let file = sanitize_path(&self.file);
        format!("{}, {}, {}, {}", dir, file, self.line, self.col)
    }

    pub fn dir(&self) -> &FilePathBuf {
        &self.dir
    }

    pub fn file(&self) -> &FilePathBuf {
        &self.file
    }

    pub fn line(&self) -> usize {
        self.line
    }

    pub fn col(&self) -> usize {
        self.col
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Effect {
    // Location of caller (Rust path::to::fun)
    caller_loc: EffectPathLoc,
    // Callee (effect) function, e.g. libc::sched_getaffinity
    callee: Path,
    // Effect pattern -- prefix of callee (effect), e.g. libc
    pattern: Option<Pattern>,
    // Location of call (Directory, file, line)
    call_loc: SrcLoc,
}

impl Effect {
    pub fn new(
        caller: String,
        callee: String,
        filepath: &FilePath,
        mod_scope: &[String],
        line: usize,
        col: usize,
    ) -> Self {
        let caller_loc = EffectPathLoc::new(filepath, mod_scope, caller);
        let callee = Path::new_owned(callee);
        let pattern = None;
        let call_loc = SrcLoc::new(filepath, line, col);
        Self { caller_loc, callee, pattern, call_loc }
    }

    pub fn caller_path(&self) -> &str {
        self.caller_loc.caller.as_str()
    }
    pub fn callee_path(&self) -> &str {
        self.callee.as_str()
    }
    /// Get the caller and callee as full paths
    pub fn caller_callee(&self) -> (&str, &str) {
        (self.caller_path(), self.callee_path())
    }

    pub fn set_pattern(&mut self, pat: Pattern) {
        self.pattern = Some(pat)
    }

    pub fn csv_header() -> &'static str {
        "crate, caller, callee, pattern, dir, file, line, col"
    }
    pub fn to_csv(&self) -> String {
        let caller_loc_csv = self.caller_loc.to_csv();
        let callee = sanitize_comma(self.callee.as_str());
        let pattern = self.pattern.as_ref().map(|p| p.as_str()).unwrap_or("[none]");
        let pattern = sanitize_comma(pattern);
        let call_loc_csv = self.call_loc.to_csv();

        format!("{}, {}, {}, {}", caller_loc_csv, callee, pattern, call_loc_csv)
    }

    pub fn pattern(&self) -> &Option<Pattern> {
        &self.pattern
    }

    pub fn call_loc(&self) -> &SrcLoc {
        &self.call_loc
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct FnDec {
    src_loc: SrcLoc,
    fn_name: Ident,
}
impl FnDec {
    pub fn new<S>(decl_span: &S, filepath: &FilePath, fn_name: String) -> Self
    where
        S: Spanned,
    {
        let line = decl_span.span().start().line;
        let col = decl_span.span().start().column;
        let src_loc = SrcLoc::new(filepath, line, col);
        let fn_name = Ident::new_owned(fn_name);
        Self { src_loc, fn_name }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct BlockDec {
    src_loc: SrcLoc,
}
impl BlockDec {
    pub fn new<S>(block_span: &S, filepath: &FilePath) -> Self
    where
        S: Spanned,
    {
        let line = block_span.span().start().line;
        let col = block_span.span().start().column;
        let src_loc = SrcLoc::new(filepath, line, col);
        Self { src_loc }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ImplDec {
    src_loc: SrcLoc,
    tr_name: Ident,
    // for_name: Ident
    // tr_loc: SrcLoc
}
impl ImplDec {
    pub fn new<S>(impl_span: &S, filepath: &FilePath, tr_name: String) -> Self
    where
        S: Spanned,
    {
        let line = impl_span.span().start().line;
        let col = impl_span.span().start().column;
        let src_loc = SrcLoc::new(filepath, line, col);
        let tr_name = Ident::new_owned(tr_name);
        Self { src_loc, tr_name }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TraitDec {
    src_loc: SrcLoc,
    tr_name: Ident,
}
impl TraitDec {
    pub fn new<S>(trait_span: &S, filepath: &FilePath, tr_name: String) -> Self
    where
        S: Spanned,
    {
        let line = trait_span.span().start().line;
        let col = trait_span.span().start().column;
        let src_loc = SrcLoc::new(filepath, line, col);
        let tr_name = Ident::new_owned(tr_name);
        Self { src_loc, tr_name }
    }
}

#[test]
fn test_csv_header() {
    assert!(Effect::csv_header().starts_with(EffectPathLoc::csv_header()));
    assert!(Effect::csv_header().ends_with(SrcLoc::csv_header()));
}
