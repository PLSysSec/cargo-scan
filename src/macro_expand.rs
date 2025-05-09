use super::ident::IdentPath;
use crate::effect::SrcLoc;
use crate::offset_maping::{MacroExpansionContext, OffsetMapping};
use crate::resolution::name_resolution::Resolver;
use crate::resolution::resolve::FileResolver;
use crate::scanner::{ScanResults, Scanner};
use anyhow::{anyhow, Context, Result};
use log::debug;
use ra_ap_hir::db::ExpandDatabase;
use ra_ap_ide::RootDatabase;
use ra_ap_ide_db::base_db::SourceDatabaseExt;
use ra_ap_ide_db::syntax_helpers::insert_whitespace_into_node::insert_ws_into;
use ra_ap_syntax::ast::{HasName, MacroCall};
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode};
use ra_ap_vfs::FileId;
use std::collections::{HashMap, HashSet};
use std::path::Path as FilePath;
use std::sync::Arc;
use ra_ap_ide::LineIndex;
pub enum ParseResult {
    File(syn::File),
}

const IGNORED_MACROS: &[&str] = &[
    "println",
    "eprintln",
    "dbg",
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "todo",
    "unimplemented",
    "cfg",
    "cfg_attr",
    "compile_error",
    "info",
    "warn",
    "error",
    "trace",
    "debug",
    "json",
    "Serialize",
    "Deserialize",
    "tracing",
    "tracing::info",
    "tracing::warn",
    "tracing::debug",
    "tracing::trace",
    "arg",
    "command",
    "test",
    "bench",
    "vec",
];

pub fn try_parse_expansion(expansion: &str) -> Result<ParseResult> {
    let mut error: Vec<_> = Vec::new();
    if let Ok(parsed_wrapped_file) = syn::parse_file(&expansion) {
        return Ok(ParseResult::File(parsed_wrapped_file));
    }
    error.push(syn::parse_file(expansion).err());
    // If none of the parsers worked, return the raw input for debugging
    Err(anyhow!("Failed to parse expansion: {:#?}", error))
}

pub fn handle_macro_expansion(
    crate_name: &str,
    filepath: &FilePath,
    resolver: &Resolver,
    macro_scan_results: &mut ScanResults,
    sinks: HashSet<IdentPath>,
    enabled_cfg: &HashMap<String, Vec<String>>,
) -> Result<()> {
    let current_file_id =
        resolver.find_file_id(filepath).context("cannot find current file id")?;
    let syntax = resolver.db().parse_or_expand(current_file_id.into());
    let file_resolver_expand = FileResolver::new(crate_name, resolver, filepath, None)?;
    let ignored_macros: HashSet<&str> = IGNORED_MACROS.iter().cloned().collect();
    for macro_call in find_macro_calls(&syntax) {
        let macro_name;
        if let Some(path) = macro_call.path() {
            macro_name = path.syntax().text().to_string();
            if ignored_macros.contains(macro_name.as_str()) {
                debug!("Ignored macro call: {}", macro_name);
                continue;
            }
        } else {
            debug!("Failed to resolve macro path.");
            continue;
        }
        let canonical_path = match file_resolver_expand.resolve_macro_def_path(&macro_call) {
            Some(cp) => cp,
            None => continue,
        };
        let text_range = macro_call.syntax().text_range();
        let line_index = LineIndex::new(&resolver.db().file_text(current_file_id).to_string());
        let start = line_index.line_col(text_range.start());
        let end = line_index.line_col(text_range.end());

        let macro_call_loc = SrcLoc::new(
            filepath,
            (start.line+1) as usize,
            start.col as usize,
        (end.line+1) as usize,
            end.col as usize,
        );
        if let Some((macro_file_id, expanded_syntax_node)) = file_resolver_expand.expand_macro(&macro_call)
        {
            let raw_text = expanded_syntax_node.text().to_string();
            let file_resolver = FileResolver::new(crate_name, resolver, filepath, Some(macro_file_id))?;
            let mut macro_scanner = Scanner::new(
                filepath,
                file_resolver,
                macro_scan_results,
                enabled_cfg,
            );
            macro_scanner.add_sinks(sinks.clone());
            let expansion = format(
                resolver.db(),
                macro_call
                    .syntax()
                    .parent()
                    .map(|it| it.kind())
                    .unwrap_or(SyntaxKind::MACRO_ITEMS),
                current_file_id,
                expanded_syntax_node,
            );
            let full_expansion = format!("fn {}() {{\n{}\n}}", macro_name , expansion);
            let mapping = OffsetMapping::build(&full_expansion, &raw_text);
            let ctx = MacroExpansionContext {
                line_index: Arc::new(LineIndex::new(&full_expansion)),
                offset_mapping: mapping,
            };
            resolver.set_macro_expansion_ctx(ctx);
            match try_parse_expansion(&full_expansion) {
                Ok(ParseResult::File(parsed_file)) => {
                    macro_scanner.set_current_macro_context(Some(canonical_path.to_string()));
                    macro_scanner.set_current_macro_call_loc(Some(macro_call_loc));
                    macro_scanner.scan_file(&parsed_file);
                    macro_scanner.set_current_macro_context(None);
                    macro_scanner.set_current_macro_call_loc(None);
                }
                Err(e) => {
                    debug!("Failed to parse expansion: {}", e);
                }
            }
            resolver.clear_macro_expansion_ctx();
        }
    }
    Ok(())
}

/// Get the canonical path of a function, method, or module containing the macro call.
pub fn get_canonical_path_from_ast(
    filepath: &FilePath,
    macro_call: &SyntaxNode,
    macro_name: String,
) -> Option<String> {
    let mut current_node = macro_call.clone();
    let mut path_components = Vec::new();
    let mut file_components = Vec::new();

    while let Some(parent) = current_node.parent() {
        current_node = parent;

        // Case 1: Macro inside a function
        if let Some(func) = ra_ap_syntax::ast::Fn::cast(current_node.clone()) {
            if let Some(name) = func.name() {
                path_components.push(name.text().to_string());
                break; // Stop at the function level
            }
        }

        // Case 2: Macro inside an `impl` block (method)
        if let Some(impl_block) = ra_ap_syntax::ast::Impl::cast(current_node.clone()) {
            if let Some(ty) = impl_block.self_ty() {
                path_components.push(ty.syntax().text().to_string());
            }
        }

        // Case 3: Macro at module level
        if let Some(module) = ra_ap_syntax::ast::Module::cast(current_node.clone()) {
            if let Some(name) = module.name() {
                path_components.push(name.text().to_string());
            }
        }
    }

    let filepath = filepath.to_str();
    if let Some(path) = filepath {
        let components: Vec<&str> = path.split('/').collect();
        if let Some(src_index) = components.iter().position(|&c| c == "src") {
            if src_index > 0 {
                // Insert the component before "src" at the head
                file_components.insert(0, components[src_index - 1].to_string());
                // Push all components after src to the tail
                for after_src in &components[src_index + 1..] {
                    if let Some(stripped) = after_src.strip_suffix(".rs") {
                        let without_extension = stripped;
                        file_components.push(without_extension.to_string());
                    } else {
                        file_components.push(after_src.to_string());
                    }
                }
            }
        }
    }
    path_components.reverse();
    file_components.append(&mut path_components);
    file_components.push(macro_name);
    Some(file_components.join("::"))
}

fn format(
    db: &RootDatabase,
    kind: SyntaxKind,
    file_id: FileId,
    expanded: SyntaxNode,
) -> String {
    let expansion = insert_ws_into(expanded).to_string();

    _format(db, kind, file_id, &expansion).unwrap_or(expansion)
}

fn _format(
    db: &RootDatabase,
    kind: SyntaxKind,
    file_id: FileId,
    expansion: &str,
) -> Option<String> {
    use ra_ap_ide_db::base_db::{FileLoader, SourceDatabase};
    // hack until we get hygiene working (same character amount to preserve formatting as much as possible)
    const DOLLAR_CRATE_REPLACE: &str = "__r_a_";
    let expansion = expansion.replace("$crate", DOLLAR_CRATE_REPLACE);
    let (prefix, suffix) = match kind {
        SyntaxKind::MACRO_PAT => ("fn __(", ": u32);"),
        SyntaxKind::MACRO_EXPR | SyntaxKind::MACRO_STMTS => ("fn __() {", "}"),
        SyntaxKind::MACRO_TYPE => ("type __ =", ";"),
        _ => ("", ""),
    };
    let expansion = format!("{prefix}{expansion}{suffix}");

    let &crate_id = db.relevant_crates(file_id).iter().next()?;
    let edition = db.crate_graph()[crate_id].edition;

    let mut cmd = std::process::Command::new(ra_ap_toolchain::rustfmt());
    cmd.arg("--edition");
    cmd.arg(edition.to_string());

    let mut rustfmt = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .ok()?;

    std::io::Write::write_all(&mut rustfmt.stdin.as_mut()?, expansion.as_bytes()).ok()?;

    let output = rustfmt.wait_with_output().ok()?;
    let captured_stdout = String::from_utf8(output.stdout).ok()?;

    if output.status.success() && !captured_stdout.trim().is_empty() {
        // let output = captured_stdout.replace(DOLLAR_CRATE_REPLACE, "$crate");
        let output = captured_stdout.trim().strip_prefix(prefix)?;
        let output = match kind {
            SyntaxKind::MACRO_PAT => output
                .strip_suffix(suffix)
                .or_else(|| output.strip_suffix(": u32,\n);"))?,
            _ => output.strip_suffix(suffix)?,
        };
        let trim_indent = ra_ap_stdx::trim_indent(output);
        // tracing::debug!("expand_macro: formatting succeeded");
        Some(trim_indent)
    } else {
        None
    }
}

fn find_macro_calls(syntax: &SyntaxNode) -> Vec<MacroCall> {
    syntax
        .descendants()
        .filter_map(<ra_ap_syntax::ast::MacroCall as AstNode>::cast)
        .collect()
}