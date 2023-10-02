/// Parsing module for `#[cfg(..)]` attributes.
use proc_macro2::{TokenStream, TokenTree};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum CfgOpt {
    /// eg. `#[cfg(test)]`
    Name(String),
    /// eg. `#[cfg(feature = "std")]`
    Pair { key: String, value: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum CfgPred {
    Invalid,
    Option(CfgOpt),
    All(Vec<CfgPred>),
    Any(Vec<CfgPred>),
    Not(Box<CfgPred>),
}

impl From<CfgOpt> for CfgPred {
    fn from(opt: CfgOpt) -> Self {
        CfgPred::Option(opt)
    }
}

impl CfgPred {
    pub fn parse(tokens: &TokenStream) -> CfgPred {
        parse_pred(&mut tokens.clone().into_iter()).unwrap_or(CfgPred::Invalid)
    }

    pub fn is_enabled(&self, enabled_opts: &HashMap<String, Vec<String>>) -> bool {
        match self {
            CfgPred::Invalid => false,
            CfgPred::Option(opt) => match opt {
                CfgOpt::Name(name) => enabled_opts.contains_key(name),
                CfgOpt::Pair { key, value } => {
                    let Some(values) = enabled_opts.get(key) else { return false };
                    values.contains(value)
                }
            },
            CfgPred::All(preds) => preds.iter().all(|x| x.is_enabled(enabled_opts)),
            CfgPred::Any(preds) => preds.iter().any(|x| x.is_enabled(enabled_opts)),
            CfgPred::Not(pred) => !pred.is_enabled(enabled_opts),
        }
    }
}

fn parse_pred(it: &mut dyn Iterator<Item = TokenTree>) -> Option<CfgPred> {
    let mut in_group = false;
    let mut peek_iter = it.peekable();

    let ident = match peek_iter.next() {
        Some(TokenTree::Ident(ident)) => ident.to_string(),
        None => return None,
        _ => return Some(CfgPred::Invalid),
    };

    let res = match peek_iter.peek() {
        Some(TokenTree::Punct(punct)) if punct.as_char().eq(&'=') => {
            match peek_iter.nth(1) {
                Some(TokenTree::Literal(literal)) => {
                    let value = literal.to_string().replace('\"', "");
                    CfgOpt::Pair { key: ident, value }.into()
                }
                _ => return Some(CfgPred::Invalid),
            }
        }
        Some(TokenTree::Group(group)) => {
            // peek_iter.next();
            in_group = true;
            let mut group_it = group.stream().into_iter();
            let mut preds = std::iter::from_fn(|| parse_pred(&mut group_it)).collect();
            match ident.as_str() {
                "all" => CfgPred::All(preds),
                "any" => CfgPred::Any(preds),
                "not" => CfgPred::Not(Box::new(preds.pop().unwrap_or(CfgPred::Invalid))),
                _ => CfgPred::Invalid,
            }
        }
        _ => CfgOpt::Name(ident).into(),
    };

    // Hacky workaround to advance the iterator, if
    // we parsed a group, due to the borrow checker
    if in_group {
        peek_iter.next();
    }

    // skip comma separator
    if let Some(TokenTree::Punct(punct)) = peek_iter.peek() {
        if punct.as_char().eq(&',') {
            peek_iter.next();
        }
    }

    Some(res)
}
