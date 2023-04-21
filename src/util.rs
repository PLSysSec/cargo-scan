//! Utility functions

/// Logging
/// Using simplelog for now, could switch to env_logger for more
/// configurability later
pub fn init_logging() {
    use env_logger::Builder;
    use log::LevelFilter;

    Builder::new().filter_module("cargo_scan", LevelFilter::Warn).init();
}

/// CSV utility functions
pub mod csv {
    use log::warn;
    use std::path::Path;

    pub fn sanitize_comma(s: &str) -> String {
        if s.contains(',') {
            warn!("Warning: ignoring unexpected comma when generating CSV: {s}");
        }
        s.replace(',', "")
    }
    pub fn sanitize_path(p: &Path) -> String {
        match p.to_str() {
            Some(s) => sanitize_comma(s),
            None => {
                warn!("Warning: path is invalid unicode: {:?}", p);
                sanitize_comma(&p.to_string_lossy())
            }
        }
    }
}

/// Iterator util
pub mod iter {
    use log::warn;
    use std::fmt::Display;
    use std::vec;

    /// Ignore errors, printing them to stderr
    /// useful with iter::filter_map: `my_iter.filter_map(warn_ok)`
    pub fn warn_ok<T, E: Display>(x: Result<T, E>) -> Option<T> {
        if let Some(e) = x.as_ref().err() {
            warn!("Warning: discarding error {}", e);
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
}

/// Filesystem util
pub mod fs {
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use std::path::PathBuf;
    use walkdir::{DirEntry, WalkDir};

    pub fn walk_files(p: &PathBuf) -> impl Iterator<Item = PathBuf> {
        debug_assert!(p.is_dir());
        WalkDir::new(p)
            .sort_by_file_name()
            .into_iter()
            .filter_map(super::iter::warn_ok)
            .map(DirEntry::into_path)
    }

    pub fn walk_files_with_extension<'a>(
        p: &'a PathBuf,
        ext: &'a str,
    ) -> impl Iterator<Item = PathBuf> + 'a {
        walk_files(p)
            .filter(|entry| entry.is_file())
            .filter(|entry| entry.extension().map_or(false, |x| x.to_str() == Some(ext)))
    }

    pub fn file_lines(p: &PathBuf) -> impl Iterator<Item = String> {
        let file = File::open(p).unwrap();
        let reader = BufReader::new(file).lines();
        reader.map(|line| line.unwrap())
    }
}
