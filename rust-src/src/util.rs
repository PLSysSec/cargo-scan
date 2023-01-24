/*
    Utility
*/

use std::fmt::Display;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::vec;

use walkdir::{DirEntry, WalkDir};

/*
    Iterators
*/

/// Ignore errors, printing them to stderr
/// useful with iter::filter_map: `my_iter.filter_map(warn_ok)`
pub fn warn_ok<T, E: Display>(x: Result<T, E>) -> Option<T> {
    if let Some(e) = x.as_ref().err() {
        eprintln!("Warning: discarding error {}", e);
    }
    x.ok()
}

/// Convert an iterator into one that owns all its elements
pub trait FreshIter {
    type Result: Iterator;
    fn fresh_iter(self) -> Self::Result;
}
impl<I: Iterator> FreshIter for I {
    type Result = vec::IntoIter<I::Item>;
    fn fresh_iter(self) -> Self::Result {
        self.collect::<Vec<I::Item>>().into_iter()
    }
}

/*
    Filesystem util
*/

pub fn walk_files(p: &PathBuf) -> impl Iterator<Item = PathBuf> {
    debug_assert!(p.is_dir());
    WalkDir::new(p)
        .sort_by_file_name()
        .into_iter()
        .filter_map(warn_ok)
        .map(DirEntry::into_path)
}

pub fn file_lines(p: &PathBuf) -> impl Iterator<Item = String> {
    let file = File::open(p).unwrap();
    let reader = BufReader::new(file).lines();
    reader.map(|line| line.unwrap())
}
