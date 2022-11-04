//! improve readability of generated code

use std::borrow::Cow;
use std::fmt::Write;

use regex::Captures;

/// reformats the generated code to improve readability.
pub(crate) fn format(tables: proc_macro2::TokenStream) -> Result<String, syn::Error> {
    // if this is not valid code just pass it through directly, and then we
    // can see the compiler errors
    let source_str = match rustfmt_wrapper::rustfmt(&tables) {
        Ok(s) => s,
        Err(_) => return Ok(tables.to_string()),
    };
    // convert doc comment attributes into normal doc comments
    let doc_comments = regex::Regex::new(r#"#\[doc = "(.*)"\]"#).unwrap();
    let source_str = doc_comments.replace_all(&source_str, "///$1");
    let newlines_before_docs = regex::Regex::new(r#"([;\}])\r?\n( *)(///|pub|impl|#)"#).unwrap();
    let source_str = newlines_before_docs.replace_all(&source_str, "$1\n\n$2$3");

    // add newlines after top-level items
    let re2 = regex::Regex::new(r"\r?\n\}").unwrap();
    let source_str = re2.replace_all(&source_str, "\n}\n\n");
    let source_str = manual_format_bitflags(&source_str);
    Ok(rustfmt_wrapper::rustfmt(source_str).unwrap())
}

/// rustfmt can't format `bitflags!` declarations so we do it manually -_-
///
/// This is... inelegant. Basically we manually reparse the output and
/// then manually reformat it. Is this a good idea? Time will tell.
fn manual_format_bitflags(code: &str) -> Cow<str> {
    fn bitflag_formatter(captures: &Captures) -> String {
        formatter_impl(captures.get(1).unwrap().as_str())
    }

    let re = regex::Regex::new(r"(?m)(^bitflags.*$)").unwrap();
    re.replace_all(code, bitflag_formatter)
}

fn formatter_impl(input: &str) -> String {
    let bitflags = BitFlagContents::parse(input);
    bitflags.generate()
}

/// the contents of a bitflags! declaration.
///
/// This can only be parsed from the output of `quote`.
struct BitFlagContents<'a> {
    docs: Vec<&'a str>,
    default: Option<&'a str>,
    name: &'a str,
    typ: &'a str,
    consts: Vec<BitFlagConst<'a>>,
}

/// A single const declared in a flagset
struct BitFlagConst<'a> {
    docs: Vec<&'a str>,
    name: &'a str,
    value: &'a str,
}

struct BitFlagCursor<'a>(&'a str);

impl<'a> BitFlagContents<'a> {
    fn parse(contents: &'a str) -> Self {
        let mut cursor = BitFlagCursor(contents);
        cursor.eat_decl().unwrap();
        let docs = cursor.eat_docs();
        let default = cursor.eat_derive_default();
        cursor.eat("pub struct").unwrap();
        let name = cursor.eat_word().unwrap();
        cursor.eat(":").unwrap();
        let typ = cursor.eat_word().unwrap();
        cursor.eat("{").unwrap();

        let consts = BitFlagConst::parse_all(&mut cursor);

        Self {
            docs,
            default,
            name,
            typ,
            consts,
        }
    }

    fn generate(&self) -> String {
        let mut result = String::new();
        writeln!(result, "{DECL}").unwrap();
        for doc in &self.docs {
            writeln!(result, "    ///{doc}").unwrap();
        }
        if let Some(default) = self.default {
            writeln!(result, "    {default}").unwrap();
        }
        writeln!(result, "    pub struct {}: {} {{", self.name, self.typ).unwrap();
        for c in &self.consts {
            for doc in &c.docs {
                writeln!(result, "        ///{doc}").unwrap();
            }
            writeln!(result, "        const {} = {};", c.name, c.value).unwrap();
        }
        writeln!(result, "    }}").unwrap();
        writeln!(result, "}}").unwrap();

        result
    }
}

impl<'a> BitFlagConst<'a> {
    fn parse_all(cursor: &mut BitFlagCursor<'a>) -> Vec<Self> {
        let mut result = Vec::new();
        while cursor.0.len() > 8 {
            result.push(Self::parse(cursor));
        }
        result
    }

    fn parse(cursor: &mut BitFlagCursor<'a>) -> Self {
        let docs = cursor.eat_docs();
        cursor.eat("const").unwrap();
        let name = cursor.eat_word().unwrap();
        cursor.eat("=").unwrap();
        let value = cursor.eat_word().unwrap();
        cursor.eat(";").unwrap();
        Self { docs, name, value }
    }
}

static DECL: &str = "bitflags::bitflags! {";

impl<'a> BitFlagCursor<'a> {
    fn eat_decl(&mut self) -> Option<&'a str> {
        self.eat(DECL)
    }

    fn eat(&mut self, pat: &str) -> Option<&'a str> {
        self.eat_spaces();
        if self.0.starts_with(pat) {
            let result = &self.0[..pat.len()];
            self.advance(pat.len());
            return Some(result);
        }
        None
    }

    fn advance(&mut self, len: usize) {
        self.0 = &self.0[len..];
    }

    fn eat_spaces(&mut self) {
        let len = self.0.bytes().take_while(|b| *b == b' ').count();
        self.advance(len);
    }

    fn eat_word(&mut self) -> Option<&'a str> {
        self.eat_spaces();
        let next_space = self.0.find(' ')?;
        let word = &self.0[..next_space];
        self.advance(next_space);
        Some(word)
    }

    fn eat_docs(&mut self) -> Vec<&'a str> {
        let mut docs = Vec::new();
        while let Some(doc) = self.eat_doc_string() {
            docs.push(doc);
        }
        docs
    }

    fn eat_derive_default(&mut self) -> Option<&'a str> {
        static DEFAULT: &str = "# [derive (Default)]";
        self.eat_spaces();
        if self.eat(DEFAULT).is_some() {
            return Some("#[derive(Default)]");
        }
        None
    }

    /// if a docstring is present, return the string itself with quotes removed
    fn eat_doc_string(&mut self) -> Option<&'a str> {
        static DOC_HEADER: &str = "# [doc = \"";
        self.eat_spaces();
        if self.0.starts_with(DOC_HEADER) {
            self.advance(DOC_HEADER.len());
            let idx = self.0.find("\"]").unwrap();
            let result = &self.0[..idx];
            self.advance(idx + 2);
            return Some(result);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitflags_formatting() {
        static INPUT: &str = r#"bitflags::bitflags! { # [doc = " See [ValueRecord]"] pub struct ValueFormat : u16 { # [doc = " Includes horizontal adjustment for placement"] const X_PLACEMENT = 0x0001 ; # [doc = " Includes vertical adjustment for placement"] const Y_PLACEMENT = 0x0002 ; } }"#;
        static OUTPUT: &str = "\
bitflags::bitflags! {
    /// See [ValueRecord]
    pub struct ValueFormat: u16 {
        /// Includes horizontal adjustment for placement
        const X_PLACEMENT = 0x0001;
        /// Includes vertical adjustment for placement
        const Y_PLACEMENT = 0x0002;
    }
}
";

        let output = formatter_impl(INPUT);
        assert_eq!(output, OUTPUT);
        static INPUT_WITH_DEFAULT: &str = r#"bitflags::bitflags! { # [doc = " See [ValueRecord]"] # [derive (Default)] pub struct ValueFormat : u16 { # [doc = " Includes horizontal adjustment for placement"] const X_PLACEMENT = 0x0001 ; # [doc = " Includes vertical adjustment for placement"] const Y_PLACEMENT = 0x0002 ; } }"#;

        // ensure we can also handle the presence of this attribute
        assert!(formatter_impl(INPUT_WITH_DEFAULT).contains("#[derive(Default)]"));
    }
}
