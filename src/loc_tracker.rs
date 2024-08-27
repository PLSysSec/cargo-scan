//! Abstract tracker for calculating total lines of code (LoC)
//! in a sequence of code blocks.
//!
//! Code blocks are added through the interface add(s) for s: Spanned.
//!
//! Important note:
//! - The "length" of each block is defined to be the end line, minus the start line,
//!   plus one if the excerpt starts and ends on the same line.

use syn::spanned::Spanned;

/// Lines of Code tracker
#[derive(Debug, Default)]
pub struct LoCTracker {
    instances: usize,
    lines: usize,
    zero_size_lines: usize,
}
impl LoCTracker {
    /// Create an empty tracker
    pub fn new() -> Self {
        Default::default()
    }

    /// Add a syn Spanned object
    pub fn add<S: Spanned>(&mut self, s: S) {
        self.instances += 1;
        let start = s.span().start().line;
        let end = s.span().end().line;
        if start == end {
            self.zero_size_lines += 1;
        } else {
            // Could add 1 here, but we choose to instead
            // track zero-sized lines separately
            self.lines += end - start;
        }
    }

    /// Return true if no spans were added
    pub fn is_empty(&self) -> bool {
        self.instances == 0
    }

    /// Get number of instances added
    pub fn get_instances(&self) -> usize {
        self.instances
    }

    /// Get total lines of code added
    pub fn get_loc(&self) -> usize {
        self.lines + self.zero_size_lines
    }

    /// Summary as a CSV
    pub fn as_csv(&self) -> String {
        format!("{}, {}", self.get_instances(), self.get_loc())
    }

    // Header for CSV output (unused)
    // pub fn csv_header() -> &'static str {
    //     "Instances, LoC"
    // }
}
