//! Hard-coded list of function patterns of interest, a.k.a. sinks.

use crate::ident::Ident;

use super::ident::{CanonicalPath, IdentPath, Pattern};

use log::warn;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fmt::{self, Display},
};

// TODO: Convert these examples to canonical paths
/// Hard-coded list of sink patterns
const SINK_PATTERNS: &[&str] = &[
    "std::arch",
    "std::backtrace",
    "std::env",
    "std::ffi",
    "std::fs",
    "std::intrinsics",
    "std::io",
    "std::mem",
    "std::net",
    "std::os",
    "std::path",
    "std::panic",
    "std::process",
    "std::simd",
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

impl Display for Sink {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Sink {
    /// Get the sink pattern matching a callee.
    pub fn new_match(callee: &CanonicalPath, sinks: &HashSet<IdentPath>) -> Option<Self> {
        let mut result = None;
        for pat_raw in sinks {
            let pat = Pattern::new(pat_raw.as_str());
            if callee.matches(&pat) {
                if let Some(x) = result {
                    warn!(
                    "Found multiple patterns of interest for {} (overwriting {} with {})",
                    callee, x, pat
                );
                }
                result = Some(pat)
            }
        }
        Some(Self(result?))
    }

    pub fn first_ident(&self) -> Option<Ident> {
        self.0.first_ident()
    }

    /// convert to str
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn default_sinks() -> HashSet<IdentPath> {
        SINK_PATTERNS.iter().map(|x| IdentPath::new(x)).collect::<HashSet<_>>()
    }
}
