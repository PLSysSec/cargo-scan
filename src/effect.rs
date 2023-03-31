/*
    This module defines the core data model for Effects.

    The main types are:
    - Effect, which represents an abstract effect
    - EffectInstance, which represents an instance of an effect in source code
    - EffectBlock, which represents a block of source code which may contain
      zero or more effects (such as an unsafe block).
*/

use super::ident::{Ident, Path};
use super::sink::Sink;
use super::util::{csv, infer};

use serde::{Deserialize, Serialize};
use std::path::{Path as FilePath, PathBuf as FilePathBuf};
use syn::spanned::Spanned;

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

        // TODO: Find the fully qualified name before we create the EffectInstance/ModPathLoc
        let crt_string = infer::infer_crate(filepath);
        let crt = Ident::new(&crt_string);

        // Infer module
        let mut post_src = infer::infer_module(filepath);

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
        let crt = csv::sanitize_comma(self.crt.as_str());
        let path = csv::sanitize_comma(self.path.as_str());
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
    start_line: usize,
    start_col: usize,
    end_line: usize,
    end_col: usize,
}

impl SrcLoc {
    pub fn new(
        filepath: &FilePath,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> Self {
        // TBD: use unwrap_or_else
        let dir = filepath.parent().unwrap().to_owned();
        let file = FilePathBuf::from(filepath.file_name().unwrap());
        Self { dir, file, start_line, start_col, end_line, end_col }
    }

    pub fn from_span<S>(filepath: &FilePath, span: &S) -> Self
    where
        S: Spanned,
    {
        let span_start = span.span().start();
        let span_end = span.span().end();

        let start_line = span_start.line;
        let start_col = span_start.column;
        let end_line = span_end.line;
        let end_col = span_end.column;

        Self::new(filepath, start_line, start_col, end_line, end_col)
    }

    pub fn sub1(&self) -> Self {
        let mut res = self.clone();
        res.start_line -= 1;
        res.end_line -= 1;
        res
    }

    pub fn csv_header() -> &'static str {
        "dir, file, line, col"
    }

    pub fn to_csv(&self) -> String {
        let dir = csv::sanitize_path(&self.dir);
        let file = csv::sanitize_path(&self.file);
        format!("{}, {}, {}, {}", dir, file, self.start_line, self.start_col)
    }

    pub fn dir(&self) -> &FilePathBuf {
        &self.dir
    }

    pub fn file(&self) -> &FilePathBuf {
        &self.file
    }

    pub fn start_line(&self) -> usize {
        self.start_line
    }

    pub fn start_col(&self) -> usize {
        self.start_col
    }

    pub fn end_line(&self) -> usize {
        self.end_line
    }

    pub fn end_col(&self) -> usize {
        self.end_col
    }
}

/*
    Data model for effects
*/

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FFICall {
    fn_name: Ident,
    callsite: SrcLoc,
    dec_loc: SrcLoc,
}
impl FFICall {
    pub fn new<S1, S2>(
        callsite: &S1,
        dec_loc: &S2,
        filepath: &FilePath,
        fn_name: String,
    ) -> Self
    where
        S1: Spanned,
        S2: Spanned,
    {
        let fn_name = Ident::new_owned(fn_name);
        let callsite = SrcLoc::from_span(filepath, callsite);
        let dec_loc = SrcLoc::from_span(filepath, dec_loc);
        Self { fn_name, callsite, dec_loc }
    }
}

/// Type representing a single effect.
/// For us, this can be any function call to some dangerous function:
/// - a sink pattern in the standard library
/// - an FFI call
/// - an unsafe operation such as a pointer dereference
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Effect {
    /// Function call (callee path) matching a sink pattern
    SinkCall(Sink),
    /// FFI call
    FFICall(Path),
    /// Unsafe operation, e.g. pointer deref
    /// see: https://doc.rust-lang.org/nomicon/what-unsafe-does.html
    UnsafeOp,
    /// Other function call -- not dangerous
    OtherCall,
}
impl Effect {
    fn sink_pattern(&self) -> Option<&Sink> {
        match self {
            Self::SinkCall(s) => Some(s),
            Self::FFICall(_) => None,
            Self::UnsafeOp => None,
            Self::OtherCall => None,
        }
    }

    fn simple_str(&self) -> &str {
        match self {
            Self::SinkCall(s) => s.as_str(),
            Self::FFICall(_) => "[FFI]",
            Self::UnsafeOp => "[Unsafe]",
            Self::OtherCall => "[None]",
        }
    }

    fn to_csv(&self) -> String {
        csv::sanitize_comma(self.simple_str())
    }
}

/// Type representing an Effect instance, with complete context.
/// This includes a field for which Effect it is an instance of;
/// it does not include cases for EffectBlocks, which are tracked separately.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EffectInstance {
    // TODO: Fold caller_loc and call_loc into EffectBlock
    /// Path to the caller function or module scope (Rust path::to::fun)
    caller_loc: ModPathLoc,
    /// Location of call or other effect (Directory, file, line)
    call_loc: SrcLoc,

    /// Callee (effect) function, e.g. libc::sched_getaffinity
    callee: Path,

    /// EffectInstance type
    /// If Sink, this includes the effect pattern -- prefix of callee (effect), e.g. libc.
    eff_type: Effect,
}

impl EffectInstance {
    pub fn new_call<S>(
        filepath: &FilePath,
        mod_scope: &[String],
        caller: String,
        callee: String,
        callsite: &S,
        is_unsafe: bool,
        ffi: Option<Path>,
    ) -> Self
    where
        S: Spanned,
    {
        let caller_loc = ModPathLoc::new_fn(filepath, mod_scope, caller);
        // TODO: This is super jank, it should be infered properly during scanning
        let callee = if callee.contains("::") {
            // The callee is (probably) already fully qualified
            Path::new_owned(callee)
        } else {
            let prefix = infer::fully_qualified_prefix(filepath);
            Path::new_owned(format!("{}::{}", prefix, callee))
        };
        let eff_type = if let Some(pat) = Sink::new_match(&callee) {
            // TODO bug
            if ffi.is_some() {
                eprintln!("Warning: found sink stdlib pattern also matching an FFI call")
            }
            Effect::SinkCall(pat)
        } else if let Some(ffi) = ffi {
            debug_assert!(is_unsafe);
            Effect::FFICall(ffi)
        } else if is_unsafe {
            Effect::UnsafeOp
        } else {
            Effect::OtherCall
        };
        let call_loc = SrcLoc::from_span(filepath, callsite);
        Self { caller_loc, call_loc, callee, eff_type }
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
        let callee = csv::sanitize_comma(self.callee.as_str());
        let effect = self.eff_type.to_csv();
        let call_loc_csv = self.call_loc.to_csv();

        format!("{}, {}, {}, {}", caller_loc_csv, callee, effect, call_loc_csv)
    }

    pub fn eff_type(&self) -> &Effect {
        &self.eff_type
    }

    pub fn pattern(&self) -> Option<&Sink> {
        self.eff_type.sink_pattern()
    }

    pub fn is_ffi(&self) -> bool {
        matches!(self.eff_type, Effect::FFICall(_))
    }

    pub fn is_unsafe_op(&self) -> bool {
        matches!(self.eff_type, Effect::UnsafeOp)
    }

    pub fn is_dangerous(&self) -> bool {
        !matches!(self.eff_type, Effect::OtherCall)
    }

    pub fn call_loc(&self) -> &SrcLoc {
        &self.call_loc
    }
}

/*
    Data model for effect blocks (unsafe blocks, functions, and impls)
*/

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FnDec {
    pub src_loc: SrcLoc,
    pub fn_name: Path,
}
impl FnDec {
    pub fn new<S>(filepath: &FilePath, decl_span: &S, fn_name: String) -> Self
    where
        S: Spanned,
    {
        let src_loc = SrcLoc::from_span(filepath, decl_span);
        let fn_name = Path::new_owned(fn_name);
        Self { src_loc, fn_name }
    }
}

/// Type representing a *block* of zero or more dangerous effects.
/// The block can be:
/// - an expression enclosed by `unsafe { ... }`
/// - a normal function decl `fn foo(args) { ... }`
/// - an unsafe function decl `unsafe fn foo(args) { ... }`
///
/// It also contains a Vector of effects inside the block.
/// However, note that the vector could be empty --
/// we don't currently enumerate all the "bad"
/// things unsafe code could do as individual effects, such as
/// pointer derefs etc.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EffectBlock {
    // TODO: Include the span in the EffectBlock
    // TODO: Include the ident of the enclosing function
    src_loc: SrcLoc,
    block_type: BlockType,
    effects: Vec<EffectInstance>,
    containing_fn: FnDec,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum BlockType {
    UnsafeExpr,
    NormalFn,
    UnsafeFn,
}
impl EffectBlock {
    pub fn new_unsafe_expr<S>(
        filepath: &FilePath,
        block_span: &S,
        containing_fn: FnDec,
    ) -> Self
    where
        S: Spanned,
    {
        let src_loc = SrcLoc::from_span(filepath, block_span);
        let block_type = BlockType::UnsafeExpr;
        let effects = Vec::new();
        Self { src_loc, block_type, effects, containing_fn }
    }
    pub fn new_fn<S>(filepath: &FilePath, decl_span: &S, fn_name: String) -> Self
    where
        S: Spanned,
    {
        let src_loc = SrcLoc::from_span(filepath, decl_span);
        let block_type = BlockType::NormalFn;
        let effects = Vec::new();
        Self {
            src_loc,
            block_type,
            effects,
            containing_fn: FnDec::new(filepath, decl_span, fn_name),
        }
    }
    pub fn new_unsafe_fn<S>(filepath: &FilePath, decl_span: &S, fn_name: String) -> Self
    where
        S: Spanned,
    {
        let src_loc = SrcLoc::from_span(filepath, decl_span);
        let block_type = BlockType::NormalFn;
        let effects = Vec::new();
        Self {
            src_loc,
            block_type,
            effects,
            containing_fn: FnDec::new(filepath, decl_span, fn_name),
        }
    }

    pub fn src_loc(&self) -> &SrcLoc {
        &self.src_loc
    }

    pub fn block_type(&self) -> &BlockType {
        &self.block_type
    }

    pub fn push_effect(&mut self, effect: EffectInstance) {
        self.effects.push(effect);
    }

    pub fn effects(&self) -> &Vec<EffectInstance> {
        &self.effects
    }

    pub fn containing_fn(&self) -> &FnDec {
        &self.containing_fn
    }
}

/// Trait implementations
/// Since an unsafe trait impl cannot itself have any unsafe code,
/// we do not consider it to be an effect block.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TraitImpl {
    src_loc: SrcLoc,
    tr_name: Path,
}
impl TraitImpl {
    pub fn new<S>(impl_span: &S, filepath: &FilePath, tr_name: String) -> Self
    where
        S: Spanned,
    {
        let src_loc = SrcLoc::from_span(filepath, impl_span);
        let tr_name = Path::new_owned(tr_name);
        Self { src_loc, tr_name }
    }
}

/// Trait declarations
/// Since an unsafe trait declaration cannot itself have any unsafe code,
/// we do not consider it to be an effect block.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TraitDec {
    src_loc: SrcLoc,
    tr_name: Path,
}
impl TraitDec {
    pub fn new<S>(trait_span: &S, filepath: &FilePath, tr_name: String) -> Self
    where
        S: Spanned,
    {
        let src_loc = SrcLoc::from_span(filepath, trait_span);
        let tr_name = Path::new_owned(tr_name);
        Self { src_loc, tr_name }
    }
}

/*
    Unit tests
*/

#[test]
fn test_csv_header() {
    assert!(EffectInstance::csv_header().starts_with(ModPathLoc::csv_header()));
    assert!(EffectInstance::csv_header().ends_with(SrcLoc::csv_header()));
}
