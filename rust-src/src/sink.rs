/*
    Patterns of interest, a.k.a. sinks
*/

use super::effect::Effect;
use super::ident::Pattern;

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

/// Given an Effect, set its sink pattern.
// Note: this compiles regex so is inefficient and
// could be optimized
pub fn set_pattern(eff: &mut Effect) {
    for &pat_raw in SINK_PATTERNS {
        let pat = Pattern::new(pat_raw);
        let callee = eff.callee();
        if callee.matches(&pat) {
            if let Some(x) = eff.pattern() {
                eprintln!(
                    "Found multiple patterns of interest for {} (overwriting {} with {})",
                    callee, x, pat
                );
            }
            eff.set_pattern(pat);
        }
    }
}
