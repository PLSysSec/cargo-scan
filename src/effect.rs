//! This module defines the core data model for Effects.
//!
//! The main types are:
//! - Effect, which represents an abstract effect
//! - EffectInstance, which represents an instance of an effect in source code
//! - EffectBlock, which represents a block of source code which may contain
//!     zero or more effects (such as an unsafe block).

use super::ident::{CanonicalPath, IdentPath};
use super::sink::Sink;
use super::util::csv;

use log::debug;
use parse_display::{Display, FromStr};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::path::{Path as FilePath, PathBuf as FilePathBuf};
use syn;
use syn::spanned::Spanned;

/*
    Abstraction for identifying a location in the source code --
    essentially derived from syn::Span
*/

/// Data representing a source code location for some identifier, block, or expression
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
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

    pub fn add1(&mut self) {
        self.start_col += 1;
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

    pub fn filepath_string(&self) -> String {
        self.dir.join(&self.file).to_string_lossy().to_string()
    }
}

impl fmt::Display for SrcLoc {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}..{}:{}",
            self.filepath_string(),
            self.start_line,
            self.start_col,
            self.end_line,
            self.end_col
        )
    }
}

/*
    Data model for effects
*/

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
    FFICall(CanonicalPath),
    /// Unsafe function/method call
    UnsafeCall(CanonicalPath),
    /// Pointer dereference
    RawPointer(CanonicalPath),
    /// Reading a union field
    UnionField(CanonicalPath),
    /// Accessing a global mutable variable
    StaticMut(CanonicalPath),
    /// Accessing an external mutable variable
    StaticExt(CanonicalPath),
    /// Creation of function pointer
    FnPtrCreation,
    /// Closure creation
    ClosureCreation,
    /// Casting *to* a raw pointer
    /// Note: This effect isn't unsafe, and is turned off by default (not included
    /// in the default list of effects to care about)
    RawPtrCast,
}
impl Effect {
    fn sink_pattern(&self) -> Option<&Sink> {
        match self {
            Self::SinkCall(s) => Some(s),
            _ => None,
        }
    }

    /// Return true if the type of unsafety is something that Rust considers unsafe.
    fn is_rust_unsafe(&self) -> bool {
        !matches!(self, Self::SinkCall(_) | Self::FnPtrCreation | Self::ClosureCreation)
    }

    fn simple_str(&self) -> &str {
        match self {
            Self::SinkCall(s) => s.as_str(),
            Self::FFICall(_) => "[FFI]",
            Self::UnsafeCall(_) => "[UnsafeCall]",
            Self::RawPointer(_) => "[PtrDeref]",
            Self::UnionField(_) => "[UnionField]",
            Self::StaticMut(_) => "[StaticMutVar]",
            Self::StaticExt(_) => "[StaticExtVar]",
            Self::FnPtrCreation => "[FnPtrCreation]",
            Self::ClosureCreation => "[ClosureCreation]",
            Self::RawPtrCast => "[RawPtrCast]",
        }
    }

    fn to_csv(&self) -> String {
        csv::sanitize_comma(self.simple_str())
    }
}

/// This is a field-less copy of Effect for easy pattern matching and passing
/// command-line arguments.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Display, FromStr)]
pub enum EffectType {
    SinkCall,
    FFICall,
    UnsafeCall,
    RawPointer,
    UnionField,
    StaticMut,
    StaticExt,
    FnPtrCreation,
    ClosureCreation,
    RawPtrCast,
}

impl EffectType {
    pub fn matches_effect(types: &[EffectType], e: &Effect) -> bool {
        match e {
            Effect::SinkCall(_) => types.contains(&EffectType::SinkCall),
            Effect::FFICall(_) => types.contains(&EffectType::FFICall),
            Effect::UnsafeCall(_) => types.contains(&EffectType::UnsafeCall),
            Effect::RawPointer(_) => types.contains(&EffectType::RawPointer),
            Effect::UnionField(_) => types.contains(&EffectType::UnionField),
            Effect::StaticMut(_) => types.contains(&EffectType::StaticMut),
            Effect::StaticExt(_) => types.contains(&EffectType::StaticExt),
            Effect::FnPtrCreation => types.contains(&EffectType::FnPtrCreation),
            Effect::ClosureCreation => types.contains(&EffectType::ClosureCreation),
            Effect::RawPtrCast => types.contains(&EffectType::RawPtrCast),
        }
    }

    pub fn unsafe_effects() -> Vec<EffectType> {
        vec![
            EffectType::SinkCall,
            EffectType::FFICall,
            EffectType::UnsafeCall,
            EffectType::RawPointer,
            EffectType::UnionField,
            EffectType::StaticMut,
            EffectType::StaticExt,
            EffectType::FnPtrCreation,
            EffectType::ClosureCreation,
        ]
    }
}

// Default effect types that we care about
// Excludes: RawPtrCast as it is not unsafe
pub const DEFAULT_EFFECT_TYPES: &[EffectType] = &[
    EffectType::SinkCall,
    EffectType::FFICall,
    EffectType::UnsafeCall,
    EffectType::RawPointer,
    EffectType::UnionField,
    EffectType::StaticMut,
    EffectType::StaticExt,
    EffectType::FnPtrCreation,
    EffectType::ClosureCreation,
];

/// Type representing an Effect instance, with complete context.
/// This includes a field for which Effect it is an instance of.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EffectInstance {
    /// Path to the caller function or module scope (Rust path::to::fun)
    caller: CanonicalPath,

    /// Location of call or other effect (Directory, file, line)
    call_loc: SrcLoc,

    /// Callee (effect) function, e.g. libc::sched_getaffinity
    callee: CanonicalPath,

    /// EffectInstance type
    /// If Sink, this includes the effect pattern -- prefix of callee (effect), e.g. libc.
    eff_type: Effect,
}

impl EffectInstance {
    /// Returns a new EffectInstance if the call matches a Sink, is an ffi call,
    /// or is an unsafe call. Regular calls are handled by the explicit call
    /// graph structure.
    pub fn new_call<S>(
        filepath: &FilePath,
        caller: CanonicalPath,
        callee: CanonicalPath,
        callsite: &S,
        is_unsafe: bool,
        ffi: Option<CanonicalPath>,
        sinks: &HashSet<IdentPath>,
    ) -> Option<Self>
    where
        S: Spanned,
    {
        // Code to classify an effect based on call site information
        let call_loc = SrcLoc::from_span(filepath, callsite);
        let eff_type = if let Some(ffi) = ffi {
            if !is_unsafe {
                // This case can occur in certain contexts, e.g. with
                // the wasm_bindgen attribute
                debug!(
                    "Found FFI callsite that wasn't marked unsafe; \
                    classifying as FFICall: \
                    {} ({}) (FFI {:?})",
                    callee, call_loc, ffi
                );
            }
            if Sink::new_match(&callee, sinks).is_some() {
                // This case occurs for many libc calls
                debug!(
                    "Found FFI callsite also matching a sink pattern; \
                    classifying as FFICall: \
                    {} ({}) (FFI {:?})",
                    callee, call_loc, ffi
                );
            }
            Some(Effect::FFICall(ffi))
        } else if let Some(pat) = Sink::new_match(&callee, sinks) {
            // callee.remove_src_loc();
            Some(Effect::SinkCall(pat))
        } else if is_unsafe {
            Some(Effect::UnsafeCall(callee.clone()))
        } else {
            None
        };
        Some(Self { caller, call_loc, callee, eff_type: eff_type? })
    }

    pub fn new_effect<S>(
        filepath: &FilePath,
        caller: CanonicalPath,
        callee: CanonicalPath,
        eff_site: &S,
        eff_type: Effect,
    ) -> Self
    where
        S: Spanned,
    {
        let call_loc = SrcLoc::from_span(filepath, eff_site);
        Self { caller, call_loc, callee, eff_type }
    }

    pub fn caller(&self) -> &CanonicalPath {
        &self.caller
    }

    pub fn caller_path(&self) -> &str {
        self.caller.as_str()
    }

    pub fn callee(&self) -> &CanonicalPath {
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
        let crt = self.caller.crate_name().to_string();
        let caller = self.caller.to_string();
        let callee = csv::sanitize_comma(self.callee.as_str());
        let effect = self.eff_type.to_csv();
        let call_loc_csv = self.call_loc.to_csv();

        format!("{}, {}, {}, {}, {}", crt, caller, callee, effect, call_loc_csv)
    }

    pub fn eff_type(&self) -> &Effect {
        &self.eff_type
    }

    pub fn pattern(&self) -> Option<&Sink> {
        self.eff_type.sink_pattern()
    }

    /// Return true if the type of unsafety is something that Rust considers unsafe.
    pub fn is_rust_unsafe(&self) -> bool {
        self.eff_type.is_rust_unsafe()
    }

    pub fn call_loc(&self) -> &SrcLoc {
        &self.call_loc
    }
}

/*
    Data model for effect blocks (unsafe blocks, functions, and impls)
*/

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Visibility {
    Public,
    Private,
}

impl From<&syn::Visibility> for Visibility {
    fn from(vis: &syn::Visibility) -> Self {
        match vis {
            syn::Visibility::Public(_) => Visibility::Public,
            // NOTE: We don't care about public restrictions, only if a function
            //       is visible to other crates
            _ => Visibility::Private,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FnDec {
    pub src_loc: SrcLoc,
    pub fn_name: CanonicalPath,
    pub vis: Visibility,
}

impl FnDec {
    pub fn new<S>(
        filepath: &FilePath,
        decl_span: &S,
        fn_name: CanonicalPath,
        vis: &syn::Visibility,
    ) -> Self
    where
        S: Spanned,
    {
        let src_loc = SrcLoc::from_span(filepath, decl_span);
        let vis = vis.into();
        Self { src_loc, fn_name, vis }
    }
}

/*
    Unit tests
*/

#[test]
fn test_csv_header() {
    assert!(EffectInstance::csv_header().ends_with(SrcLoc::csv_header()));
}
