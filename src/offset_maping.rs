use std::sync::Arc;

use ra_ap_syntax::{SourceFile, SyntaxKind, SyntaxToken};

#[derive(Debug, Clone)]
pub struct OffsetMapping {
    offset_map: Vec<Option<ra_ap_ide::TextSize>>,
}

impl OffsetMapping {
    pub fn build(formatted: &str, raw: &str) -> Self {
        let mut offset_map = vec![None; formatted.len() + 1];
        let lines: Vec<&str> = formatted.split_terminator('\n').collect();
        if lines.len() < 2 {
            return Self { offset_map };
        }
        let body_lines = &lines[1..lines.len() - 1];
        let body_text = body_lines.join("\n");
        let prefix_bytes = lines[0].len() + 1;
        let tokens = parse_tokens(&body_text);
        let mut raw_pos = 0;
        for t in tokens.iter() {
            if t.kind() == SyntaxKind::IDENT {
                let ident_text = t.text().to_string();
                let f_start: usize = t.text_range().start().into();
                let length = ident_text.len();
                if let Some(found) = raw[raw_pos..].find(&ident_text) {
                    let raw_offset = raw_pos + found;
                    for i in 0..length {
                        let f_off = prefix_bytes + f_start + i;
                        if f_off < offset_map.len() {
                            offset_map[f_off] = Some(TextSize::from((raw_offset + i) as u32));
                        }
                    }
                    raw_pos = raw_offset + length;
                } 
            }
        }

        let matched_count = offset_map.iter().filter(|x| x.is_some()).count();
        let total = offset_map.len();
        eprintln!(
            "[OffsetMapping(Brute-Search)] Matched {} of {} positions ({:.2}%)",
            matched_count,
            total,
            (matched_count as f64) / (total as f64) * 100.0
        );

        Self { offset_map }
    }

    pub fn to_raw_offset(&self, formatted_offset: usize) -> Option<TextSize> {
        if formatted_offset < self.offset_map.len() {
            self.offset_map[formatted_offset]
        } else {
            None
        }
    }
}

fn parse_tokens(code: &str) -> Vec<SyntaxToken> {
    let parse = SourceFile::parse(code);
    let node = parse.syntax_node();
    node.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| {
            let kind = t.kind();
            !(kind.is_trivia() || kind == SyntaxKind::COMMENT)
        })
        .collect()
}



// ----------------------------------------------------------------------

use ra_ap_ide::{LineIndex, TextSize};

#[derive(Debug, Clone)]
pub struct MacroExpansionContext {
    pub line_index: Arc<LineIndex>,
    pub offset_mapping: OffsetMapping,
}

impl MacroExpansionContext {
    pub fn new(formatted: &str, raw: &str) -> Self {
        let line_index = Arc::new(LineIndex::new(formatted));
        let offset_mapping = OffsetMapping::build(formatted, raw);
        Self {
            line_index,
            offset_mapping,
        }
    }
}
