#![allow(unused_variables)]

use super::effect::SrcLoc;
use super::ident::{Ident, Path};

/// Path that is fully expanded and canonical
/// e.g. if I do `use libc::foobar as baz`
/// then `baz` is a valid Path, but not a valid
/// CanonicalPath
///
/// NB: `libc` and `foobar` are valid Idents and valid Paths
/// `libc::foobar` is a Path but not an Ident
pub struct CanonicalPath(Path);

/// SrcLoc: file, line, col
/// Ident: method call or function call
/// e.g. push

/// Return canonical path which that ident resolves to
/// e.g. for push we should return `std::vec::Vec::push`
pub fn resolve_ident(s: SrcLoc, i: Ident) -> CanonicalPath {
    todo!()
}

pub fn resolve_path(s: SrcLoc, p: Path) -> CanonicalPath {
    todo!()
}
