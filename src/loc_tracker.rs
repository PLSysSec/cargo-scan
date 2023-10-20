//! Abstract tracker for calculating total lines of code (LoC)
//! in a sequence of code excerpts.
//!
//! Supports a simple interface using add(s) for s: Spanned
//! to add a code excerpt.
//!
//! Important note:
//! - Assumes that code excerpts do not overlap.
//! - If multiple code excerpts start and end on the same line, this
//!   may result in an overapproximation as get_loc() counts zero
//!   sized excerpts as one line each.

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

    /// Get lines of code lower bound
    pub fn get_loc_lb(&self) -> usize {
        self.lines
    }

    /// Get lines of code upper bound
    pub fn get_loc_ub(&self) -> usize {
        self.lines + self.zero_size_lines
    }

    /// Summary as a CSV
    pub fn as_csv(&self) -> String {
        format!("{}, {}, {}", self.get_instances(), self.get_loc_lb(), self.get_loc_ub())
    }

    /// Header for CSV output
    pub fn csv_header() -> &'static str {
        "Instances, LoC (lower bound), LoC (upper bound)"
    }
}
