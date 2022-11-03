/*
    Toy version of the url crate
*/

use std::str::Chars;

pub struct Url {
    // Components:
    // https://
    pub scheme: SchemeType,
    // www
    pub subdomain: String,
    // example
    pub domain: String,
    // com
    pub toplevel: String,
    // /2022/index.html
    pub path: String,
}

impl Url {
    /// Parse an absolute URL from a string.
    #[inline]
    pub fn parse(input: &str) -> Option<Url> {
        let parser = Parser {
            serialization: String::with_capacity(input.len()),
            _base_url: None,
            // query_encoding_override: None
            // violation_fn: None
            // context: Context::UrlParser,
        };
        parser.parse_url(input)
    }
}

pub struct Parser<'a> {
    serialization: String,
    _base_url: Option<&'a Url>,
    // query_encoding_override: EncodingOverride<'a>,
    // violation_fn: Option<&'a dyn Fn(SyntaxViolation)>,
    // context: Context,
}

impl<'a> Parser<'a> {
    pub fn parse_url(mut self, input: &str) -> Option<Url> {
        let input = input.chars();
        if let Some(remaining) = self.parse_scheme(input) {
            self.parse_with_scheme(remaining)
        } else {
            None
        }
    }

    pub fn parse_scheme<'i>(&mut self, mut input: Chars<'i>) -> Option<Chars<'i>> {
        input.clone().next()?;
        while let Some(c) = input.next() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '+' | '-' | '.' => {
                    self.serialization.push(c.to_ascii_lowercase())
                }
                ':' => return Some(input),
                _ => {
                    return None;
                }
            }
        }
        None
    }

    fn parse_with_scheme(mut self, input: Chars<'_>) -> Option<Url> {
        let scheme_end = self.serialization.len() as u32;
        let scheme_type = SchemeType::from(&self.serialization);
        self.serialization.push(':');

        match scheme_type {
            SchemeType::File => None,
            SchemeType::NotSpecial => None,
            SchemeType::SpecialNotFile => {
                let _remaining = input.clone().filter(|c| matches!(c, '/' | '\\')).count();
                self.after_double_slash(input, scheme_type, scheme_end)
            }
        }
    }

    fn after_double_slash(
        mut self,
        input: Chars<'_>,
        scheme_type: SchemeType,
        _scheme_end: u32,
    ) -> Option<Url> {
        self.serialization.push('/');
        self.serialization.push('/');
        // path state
        let _path_start = self.serialization.len() as u32;
        // cutting off here -- ignore the rest of parsing
        // just return some stuff
        self.serialization.extend(input);
        let url = Url {
            scheme: scheme_type,
            subdomain: self.serialization.clone(),
            domain: self.serialization.clone(),
            toplevel: self.serialization.clone(),
            path: self.serialization,
        };
        Some(url)
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum SchemeType {
    File,
    SpecialNotFile,
    NotSpecial,
}

impl SchemeType {
    pub fn from(s: &str) -> Self {
        match s {
            "http" | "https" | "ws" | "wss" | "ftp" => SchemeType::SpecialNotFile,
            "file" => SchemeType::File,
            _ => SchemeType::NotSpecial,
        }
    }
}
