/*
    Representation of an Effect location in source code
*/

use super::ident::{Ident, Path};
use super::sink::Sink;
use super::util::{fully_qualified_prefix, infer_crate, infer_module};

use serde::{Deserialize, Serialize};
use std::path::{Path as FilePath, PathBuf as FilePathBuf};
use syn::spanned::Spanned;

/*
    CSV utility functions
*/
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

/*
    Abstractions for identifying a location in the source code --
    either by a module/fn path, or by a file/line/column
*/

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ModPathLoc {
    // Name of crate, e.g. num_cpus
    crt: Ident,
    // Full path to module or function, e.g. num_cpus::linux::logical_cpus
    path: Path,
}
impl ModPathLoc {
    pub fn new_fn(filepath: &FilePath, mod_scope: &[String], fn_decl: String) -> Self {
        // note: tries to infer the crate name and modules from the filepath
        // TBD: use Cargo.toml to get crate name & other info

        // TODO: Find the fully qualified name before we create the Effect/EffectPath
        let crt_string = infer_crate(filepath);
        let crt = Ident::new_owned(crt_string.clone());

        // Infer module
        let mut post_src = infer_module(filepath);

        // combine crate, module scope, and file-level modules (mod_scope)
        // to form full module path
        let mut full_scope: Vec<String> = vec![crt_string];
        full_scope.append(&mut post_src);
        full_scope.extend_from_slice(mod_scope);
        full_scope.push(fn_decl);

        let path = Path::new_owned(full_scope.join("::"));
        Self { crt, path }
    }

    pub fn csv_header() -> &'static str {
        "crate, fn_decl"
    }

    pub fn to_csv(&self) -> String {
        let crt = sanitize_comma(self.crt.as_str());
        let path = sanitize_comma(self.path.as_str());
        format!("{}, {}", crt, path)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Data representing a source code location for some identifier, block, or expression
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SrcLoc {
    /// Directory in which the expression occurs
    dir: FilePathBuf,
    /// File in which the expression occurs -- in the above directory
    file: FilePathBuf,
    /// Location in which the expression occurs -- in the above file
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

/*
    Data model for effects
*/

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BlockDec {
    src_loc: SrcLoc,
    ffi_calls: Vec<FFICall>,
}
impl BlockDec {
    pub fn new<S>(block_span: &S, filepath: &FilePath) -> Self
    where
        S: Spanned,
    {
        let line = block_span.span().start().line;
        let col = block_span.span().start().column;
        let src_loc = SrcLoc::new(filepath, line, col);
        Self { src_loc, ffi_calls: Vec::new() }
    }
    pub fn get_src_loc(&self) -> &SrcLoc {
        &self.src_loc
    }
    pub fn add_ffi_call(&mut self, ffi_call: FFICall) {
        self.ffi_calls.push(ffi_call);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FnDec {
    pub src_loc: SrcLoc,
    pub fn_name: Ident,
    // TODO: should this have a list of FFI calls also?
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ImplDec {
    src_loc: SrcLoc,
    tr_name: Ident,
    // for_name: Ident
    // tr_loc: SrcLoc
    // TODO: should this have a list of FFI calls also?
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FFICall {
    fn_name: Ident,
    callsite: SrcLoc,
    dec_loc: SrcLoc,
}
impl FFICall {
    pub fn new<S>(callsite: &S, dec_loc: &S, filepath: &FilePath, fn_name: String) -> Self
    where
        S: Spanned,
    {
        let mut line = callsite.span().start().line;
        let mut col = callsite.span().start().column;
        let callsite = SrcLoc::new(filepath, line, col);
        line = dec_loc.span().start().line;
        col = dec_loc.span().start().column;
        let dec_loc = SrcLoc::new(filepath, line, col);
        let fn_name = Ident::new_owned(fn_name);
        Self { fn_name, callsite, dec_loc }
    }
}

/// Type representing a single, "atomic" effect: a function call
/// to some dangerous function.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EffectCore {
    /// Function call (callee path) matching a sink pattern
    SinkCall(Sink),
    /// FFI call
    FFICall(FFICall),
    /// Other call (callee path), not matching any sink pattern
    OtherCall,
}
impl EffectCore {
    fn sink_pattern(&self) -> Option<&Sink> {
        match self {
            Self::SinkCall(s) => Some(s),
            Self::FFICall(_) => None,
            Self::OtherCall => None,
        }
    }
    fn simple_str(&self) -> &str {
        match self {
            Self::SinkCall(s) => s.as_str(),
            Self::FFICall(_) => "[FFI call]",
            Self::OtherCall => "[none]",
        }
    }
    fn to_csv(&self) -> String {
        sanitize_comma(self.simple_str())
    }
}

/// Type representing a *block* of zero or more dangerous effects.
/// The block can be:
/// - an expression enclosed by `unsafe { ... }`
/// - an unsafe function decl `unsafe fn ...`
/// - an unsafe trait impl `unsafe impl <Trait> for <Type> ...`
///
/// We use this model because we don't currently enumerate all the "bad"
/// things unsafe code could do as individual effects.
/// This is TBD and could change later.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EffectBlock {
    UnsafeBlock(BlockDec),
    UnsafeFn(FnDec),
    UnsafeImpl(ImplDec),
}
impl EffectBlock {
    pub fn get_src_loc(&self) -> &SrcLoc {
        match self {
            Self::UnsafeBlock(b) => b.get_src_loc(),
            Self::UnsafeFn(_) => unimplemented!(),
            Self::UnsafeImpl(_) => unimplemented!(),
        }
    }
    pub fn add_ffi_call(&mut self, ffi_call: FFICall) {
        match self {
            Self::UnsafeBlock(b) => {
                b.ffi_calls.push(ffi_call);
            }
            Self::UnsafeFn(_) => unimplemented!(),
            Self::UnsafeImpl(_) => unimplemented!(),
        }
    }
}

/// Type representing an Effect instance, with complete context.
/// This includes a field for which EffectCore it is an instance of;
/// it does not include cases for EffectBlocks, which are tracked separately.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Effect {
    /// Path to the caller function or module scope (Rust path::to::fun)
    caller_loc: ModPathLoc,
    /// Location of call or other effect (Directory, file, line)
    call_loc: SrcLoc,

    /// Callee (effect) function, e.g. libc::sched_getaffinity
    callee: Path,

    /// Effect type
    /// If Sink, this includes the effect pattern -- prefix of callee (effect), e.g. libc.
    eff_type: EffectCore,
}

impl Effect {
    pub fn new_call(
        caller: String,
        callee: String,
        filepath: &FilePath,
        mod_scope: &[String],
        line: usize,
        col: usize,
    ) -> Self {
        let caller_loc = ModPathLoc::new_fn(filepath, mod_scope, caller);
        // TODO: This is super jank, it should be infered properly during scanning
        let callee = if callee.contains("::") {
            // The callee is (probably) already fully qualified
            Path::new_owned(callee)
        } else {
            let prefix = fully_qualified_prefix(filepath);
            Path::new_owned(format!("{}::{}", prefix, callee))
        };
        let eff_type = if let Some(pat) = Sink::new_match(&callee) {
            EffectCore::SinkCall(pat)
        } else {
            EffectCore::OtherCall
        };
        let call_loc = SrcLoc::new(filepath, line, col);
        Self { caller_loc, call_loc, callee, eff_type }
    }
    pub fn new_ffi_call<S>(
        callsite: &S,
        dec_loc: &S,
        filepath: &FilePath,
        fn_name: String,
        mod_scope: &[String],
        caller: String,
    ) -> Self
    where
        S: Spanned,
    {
        let caller_loc = ModPathLoc::new_fn(filepath, mod_scope, caller);
        let call = FFICall::new(callsite, dec_loc, filepath, fn_name);
        let call_loc = call.callsite.clone();
        let callee = Path::from_ident(call.fn_name.clone());
        let eff_type = EffectCore::FFICall(call);
        Effect { caller_loc, call_loc, callee, eff_type }
    }

    pub fn caller(&self) -> &Path {
        self.caller_loc.path()
    }
    pub fn caller_path(&self) -> &str {
        self.caller_loc.path().as_str()
    }
    pub fn callee(&self) -> &Path {
        &self.callee
    }
    pub fn callee_path(&self) -> &str {
        self.callee.as_str()
    }
    /// Get the caller and callee as full paths
    pub fn caller_callee(&self) -> (&str, &str) {
        (self.caller_path(), self.callee_path())
    }

    pub fn csv_header() -> &'static str {
        "crate, fn_decl, callee, effect, dir, file, line, col"
    }
    pub fn to_csv(&self) -> String {
        let caller_loc_csv = self.caller_loc.to_csv();
        let callee = sanitize_comma(self.callee.as_str());
        let effect = self.eff_type.to_csv();
        let call_loc_csv = self.call_loc.to_csv();

        format!("{}, {}, {}, {}", caller_loc_csv, callee, effect, call_loc_csv)
    }

    pub fn pattern(&self) -> Option<&Sink> {
        self.eff_type.sink_pattern()
    }

    pub fn call_loc(&self) -> &SrcLoc {
        &self.call_loc
    }
}

#[test]
fn test_csv_header() {
    assert!(Effect::csv_header().starts_with(ModPathLoc::csv_header()));
    assert!(Effect::csv_header().ends_with(SrcLoc::csv_header()));
}
