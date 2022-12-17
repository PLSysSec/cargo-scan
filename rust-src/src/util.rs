use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::vec;

pub fn file_lines(p: &PathBuf) -> impl Iterator<Item = String> {
    let file = File::open(p).unwrap();
    let reader = BufReader::new(file).lines();
    reader.map(|line| line.unwrap())
}

/// Utility trait to convert an iterator into one that owns all its elements
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
