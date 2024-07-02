use anyhow::{anyhow, Error};
use cargo_scan::{audit_file::SafetyAnnotation, effect::SrcLoc};
use lsp_types::{Location, Position, Range, Url};
use std::{path::PathBuf, str::FromStr};

/// Converts Cargo Scan's `SrcLoc` to an appropriate
/// LSP `Location` to send to the LSP client
pub fn from_src_loc(loc: &SrcLoc) -> Result<Location, Error> {
    let filepath = loc.filepath_string();
    let location = Location {
        uri: Url::from_file_path(filepath.clone()).map_err(|_| {
            anyhow!("Could not create LSP Url from filepath {}", filepath)
        })?,
        range: Range {
            start: Position {
                line: loc.sub1().start_line() as u32,
                character: loc.start_col() as u32,
            },
            end: Position {
                line: loc.sub1().end_line() as u32,
                character: loc.end_col() as u32,
            },
        },
    };

    Ok(location)
}

/// Converts an LSP `Location` received from the client
/// back to Cargo Scan's `SrcLoc`
pub fn to_src_loc(location: Location) -> Result<SrcLoc, Error> {
    let path = PathBuf::from_str(location.uri.path())?;
    let start_line = location.range.start.line as usize + 1;
    let start_col = location.range.start.character as usize;
    let end_line = location.range.end.line as usize + 1;
    let end_col = location.range.end.character as usize;

    Ok(SrcLoc::new(&path, start_line, start_col, end_line, end_col))
}

pub fn convert_annotation(annotation: String) -> SafetyAnnotation {
    match annotation.as_str() {
        "Safe" => SafetyAnnotation::Safe,
        "Unsafe" => SafetyAnnotation::Unsafe,
        "Caller-Checked" => SafetyAnnotation::CallerChecked,
        _ => SafetyAnnotation::Skipped,
    }
}
