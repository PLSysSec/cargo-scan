/*
    Hard-coded list of patterns of interest ()
    Patterns of interest, a.k.a. sinks.
*/

use super::ident::{Path, Pattern};

use serde::{Deserialize, Serialize};

/// Hard-coded list of sink patterns
const SINK_PATTERNS: &[&str] = &[
    "std::env",
    "std::fs",
    "std::net",
    "std::os",
    "std::path",
    "std::process",
    "libc",
    "winapi",
    "mio::net",
    "mio::unix",
    "tokio::fs",
    "tokio::io",
    "tokio::net",
    "tokio::process",
    "hyper::client",
    "hyper::server",
    "tokio_util::udp",
    "tokio_util::net",
    "socket2",
];

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sink(Pattern);
impl Sink {
    /// Get the sink pattern matching a callee.
    /// This uses the hardcoded list of sink patterns in SINK_PATTERNS.
    //
    // TODO: allocating new Patterns every time this is called is inefficient.
    // Use lazy_static! to create the list of pattern strings only once.
    // (Or even better, compile the patterns to a Trie or similar)
    pub fn new_match(callee: &Path) -> Option<Self> {
        let mut result = None;
        for &pat_raw in SINK_PATTERNS {
            let pat = Pattern::new(pat_raw);
            if callee.matches(&pat) {
                if let Some(x) = result {
                    eprintln!(
                    "Found multiple patterns of interest for {} (overwriting {} with {})",
                    callee, x, pat
                );
                }
                result = Some(pat)
            }
        }
        Some(Self(result?))
    }

    /// convert to str
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
