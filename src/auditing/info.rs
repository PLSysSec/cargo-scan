use std::collections::HashMap;

use anyhow::{anyhow, Result};
use clap::Parser;
use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term;
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};

use crate::ident::CanonicalPath;
use crate::{
    effect::{Effect, EffectBlock, SrcLoc},
    policy::EffectInfo,
};

#[derive(Parser, Debug)]
pub struct Config {
    #[clap(long = "lines-before", default_value_t = 4)]
    /// The number of lines before an effect to show
    lines_before_effect: u8,

    #[clap(long = "lines-after", default_value_t = 1)]
    /// The number of lines after an effect to show
    lines_after_effect: u8,
    // TODO: Add flag for if we can traverse to child packages (maybe always
    //           can now that chains are our primary auditing mechanism?)
}

impl Default for Config {
    fn default() -> Self {
        Config { lines_before_effect: 4, lines_after_effect: 1 }
    }
}

impl Config {
    pub fn new(lines_before: u8, lines_after: u8) -> Self {
        Self { lines_before_effect: lines_before, lines_after_effect: lines_after }
    }
}

pub fn print_effect_src(
    effect_origin: &EffectBlock,
    effect: &EffectInfo,
    fn_locs: &HashMap<CanonicalPath, SrcLoc>,
    config: &Config,
) -> Result<()> {
    // NOTE: The codespan lines are 0-indexed, but SrcLocs are 1-indexed
    let effect_loc = &effect.callee_loc.sub1();
    let mut full_path = effect_loc.dir().clone();
    full_path.push(effect_loc.file());

    let src_contents = std::fs::read_to_string(full_path)?;

    // Get the byte ranges for each line of the src file
    let src_lines = src_contents.split('\n');
    let mut src_linenum_ranges = HashMap::new();
    src_lines.fold((0, 0), |(lineno, byte_count), line| {
        src_linenum_ranges.insert(lineno, (byte_count, byte_count + line.len() + 1));
        (lineno + 1, byte_count + line.len() + 1)
    });

    // calculate the byte ranges for the effect
    let start_effect_line = effect_loc.start_line();
    let end_effect_line = effect_loc.end_line();
    let bounded_start_line =
        start_effect_line.saturating_sub(config.lines_before_effect as usize);
    let bounded_end_line = std::cmp::min(
        end_effect_line + config.lines_after_effect as usize,
        src_linenum_ranges.len(),
    );

    let surrounding_start = src_linenum_ranges.get(&bounded_start_line).unwrap().0;
    let surrounding_end = src_linenum_ranges.get(&bounded_end_line).unwrap().1;
    let effect_start = src_linenum_ranges.get(&start_effect_line).unwrap().0;
    let effect_end = src_linenum_ranges.get(&end_effect_line).unwrap().1;

    // TODO: cache files?
    let mut files = SimpleFiles::new();
    let file_id = files.add(format!("{}", effect_loc.file().display()), src_contents);

    // If the labels don't include the function signature, include it as
    // another label
    // NOTE: The codespan lines are 0-indexed, but SrcLocs are 1-indexed
    let mut labels = match fn_locs.get(&effect.caller_path).map(|x| x.sub1()) {
        Some(loc)
            if loc.start_line() < bounded_start_line
                && loc.end_line() < bounded_start_line =>
        {
            // The signature is entirely outside the current label range, so add
            // a new label with the signature
            let sig_start = src_linenum_ranges.get(&loc.start_line()).unwrap().0;
            let sig_end = src_linenum_ranges.get(&loc.end_line()).unwrap().1;
            vec![
                Label::primary(file_id, effect_start..effect_end),
                Label::secondary(file_id, sig_start..sig_end),
                Label::secondary(file_id, surrounding_start..surrounding_end),
            ]
        }
        Some(loc) if loc.start_line() < bounded_start_line => {
            // The start of the signature is outside the current label range, so
            // extend the surrounding range to include the start of the function
            // signature
            let sig_start = src_linenum_ranges.get(&loc.start_line()).unwrap().0;
            vec![
                Label::primary(file_id, effect_start..effect_end),
                Label::secondary(file_id, sig_start..surrounding_end),
            ]
        }
        Some(_) | None => {
            // The function signature is already included in the label range or
            // we couldn't find the function definition
            vec![
                Label::primary(file_id, effect_start..effect_end),
                Label::secondary(file_id, surrounding_start..surrounding_end),
            ]
        }
    };

    let label_msg = if effect_origin.containing_fn().fn_name == effect.caller_path {
        // We are in the original function, so print all the effects in the
        // EffectBlock
        effect_origin
            .effects()
            .iter()
            .filter_map(|x| match x.eff_type() {
                Effect::SinkCall(sink) => Some(format!("sink call: {}", sink)),
                Effect::FFICall(call) => Some(format!("ffi call: {}", call)),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        "call safety marked as callee-checked".to_string()
    };
    let l = labels.remove(0);
    labels.insert(0, l.with_message(label_msg));

    // construct the codespan diagnostic
    let diag = Diagnostic::help().with_code("Audit location").with_labels(labels);

    let writer = StandardStream::stderr(ColorChoice::Always);
    let config = codespan_reporting::term::Config::default();

    // Print the information to the user
    term::emit(&mut writer.lock(), &config, &files, &diag)?;

    Ok(())
}

struct CallStackInfo {
    fn_string: Option<String>,
    filename: String,
    lineno: usize,
}

impl CallStackInfo {
    fn new(fn_string: Option<String>, filename: String, lineno: usize) -> Self {
        Self { fn_string, filename, lineno }
    }
}

fn fn_decl_info(fn_loc: &SrcLoc) -> Result<CallStackInfo> {
    let mut full_path = fn_loc.dir().clone();
    full_path.push(fn_loc.file());

    let src_contents = std::fs::read_to_string(full_path)?;

    // TODO: Print the full definition if it spans multiple lines
    let mut src_lines = src_contents.splitn(fn_loc.start_line() + 1, '\n');
    let src_fn_loc = src_lines
        .nth(fn_loc.start_line() - 1)
        .ok_or_else(|| anyhow!("Source lineno past end of file"))?;

    // TODO: Capture just the function name
    let res = CallStackInfo::new(
        Some(src_fn_loc.split('{').next().unwrap().trim().to_string()),
        format!("{}", fn_loc.dir().to_string_lossy()),
        fn_loc.start_line(),
    );
    Ok(res)
}

fn missing_fn_decl_info(effect_loc: &SrcLoc) -> CallStackInfo {
    let mut path_list = effect_loc.dir().clone();
    path_list.push(effect_loc.file());
    let full_path = path_list.join("/");
    let full_path_str = full_path.to_string_lossy().to_string();

    CallStackInfo::new(None, full_path_str, effect_loc.start_line())
}

fn print_call_stack_infos(stack: Vec<CallStackInfo>) {
    let missing_fn_str = "Missing fn decl";
    // TODO: Colorize
    for CallStackInfo { fn_string, filename, lineno } in stack {
        println!("{}:{}", filename, lineno + 1);
        match fn_string {
            Some(f) => println!("    {}", f),
            None => println!("    {}", missing_fn_str),
        };
    }
}

fn print_call_stack(
    curr_effect: &EffectInfo,
    effect_history: &[&EffectInfo],
    fn_locs: &HashMap<CanonicalPath, SrcLoc>,
) -> Result<()> {
    if !effect_history.is_empty() {
        let mut call_stack_infos = vec![];
        // TODO: Colorize
        println!("EffectInstance call stack:");
        let call_info = match fn_locs.get(&curr_effect.caller_path) {
            Some(fn_loc) => fn_decl_info(fn_loc)?,
            None => missing_fn_decl_info(&curr_effect.callee_loc),
        };
        call_stack_infos.push(call_info);

        for e in effect_history.iter().rev() {
            let call_info = match fn_locs.get(&e.caller_path) {
                Some(fn_loc) => fn_decl_info(fn_loc)?,
                None => missing_fn_decl_info(&e.callee_loc),
            };
            call_stack_infos.push(call_info);
        }

        print_call_stack_infos(call_stack_infos);
    }

    Ok(())
}

pub fn print_effect_info(
    orig_effect: &EffectBlock,
    curr_effect: &EffectInfo,
    effect_history: &[&EffectInfo],
    fn_locs: &HashMap<CanonicalPath, SrcLoc>,
    config: &Config,
) -> Result<()> {
    println!();
    println!("=================================================");
    print_call_stack(curr_effect, effect_history, fn_locs)?;
    println!();
    print_effect_src(orig_effect, curr_effect, fn_locs, config)?;
    Ok(())
}
