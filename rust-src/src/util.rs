use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

pub fn file_lines(p: &PathBuf) -> impl Iterator<Item = String> {
    let file = File::open(p).unwrap();
    let reader = BufReader::new(file).lines();
    reader.map(|line| line.unwrap())
}
