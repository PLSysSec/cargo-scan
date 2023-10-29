pub mod audit_chain;
pub mod audit_file;
pub mod auditing;
pub mod download_crate;
pub mod effect;
pub mod ident;
pub mod loc_tracker;
pub mod scan_stats;
pub mod scanner;
pub mod sink;
pub mod util;

// Name resolution
pub mod hacky_resolver;
pub mod name_resolution;
pub mod resolve;

// Attribute parser
mod attr_parser;
