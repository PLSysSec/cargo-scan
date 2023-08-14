//! Abstract tracker for calculating total lines of code (LoC)
//! in a sequence of code excerpts.
//!
//! Supports a simple interface using add(s) for s: Spanned
//! to add a code excerpt.
//!
//! Important note:
//! - Assumes that code excerpts do not overlap.
//! - If multiple code excerpts start and end on the same line, this
//!   may result in an overapproximation as as_loc() counts zero
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

    /// Attempt to summarize the tracker as a single "lines of code" number
    ///
    /// This overapproximates by counting zero-sized lines as a full line.
    pub fn as_loc(&self) -> usize {
        self.lines + self.zero_size_lines
    }
}
