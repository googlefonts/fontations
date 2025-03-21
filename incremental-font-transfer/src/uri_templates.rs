//! Implementation of the specific variant of URI template expansion required by the IFT specification.
//!
//! Context: <https://w3c.github.io/IFT/Overview.html#uri-templates>
//!
//! In IFT RFC6570 style uri templates are used, however the IFT specification restricts template syntax
//! to a subset (level 1 with a predefined set of variables) of the full RFC6570 syntax. This implements
//! a URI template expander that adheres to the IFT specific requirements.
//!
//! By implementing our own we avoid pulling in a much larger general purpose template expansion library
//! and improve performance versus a more general implementation.
use data_encoding::BASE64URL;
use data_encoding_macro::new_encoding;
use std::fmt::Write;

use crate::patchmap::PatchId;

/// Tracks what part of the URI template is currently being parsed.
enum ParseState {
    /// Currently parsing literal values (https://datatracker.ietf.org/doc/html/rfc6570#section-3.1)
    Literal,

    /// Currently validating a percent encoding (%XX) instance present in literals.
    LiteralPercentEncoded(Digit),

    /// Currently parsing the variable name of an expression (https://datatracker.ietf.org/doc/html/rfc6570#section-3.2)
    ///
    /// Variable tracks the state of variable name matching.
    Expression(Variable),
}

/// Represents the process of matching one of the predefined variable names: id, id64, d1, d2, d3, d4
enum Variable {
    Begin,
    I,
    ID,
    ID6,
    ID64,
    D,
    DX(u8),
}

/// Which digit of a percent encoding we're on
enum Digit {
    One,
    Two,
}

#[derive(Default)]
struct OutputBuffer(String);

/// Indicates a malformed URI template was encountered.
///
/// More info: <https://datatracker.ietf.org/doc/html/rfc6570#section-3>
#[derive(Debug, PartialEq, Eq)]
pub struct UriTemplateError;

impl std::fmt::Display for UriTemplateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Invalid URI template encountered.")
    }
}

impl std::error::Error for UriTemplateError {}

/// Implements uri template expansion from incremental font transfer.
///
/// Specification: <https://w3c.github.io/IFT/Overview.html#uri-templates>
///
/// IFT uri templates are a subset of the more general <https://datatracker.ietf.org/doc/html/rfc6570>
/// uri templates. Notably, only level one substitution expressions are supported and there are a fixed
/// set of variables used in the expansion (id, id64, d1, d2, d3, and d4).
pub(crate) fn expand_template(
    template_string: &str,
    patch_id: &PatchId,
) -> Result<String, UriTemplateError> {
    let (id_string, id64_string) = match &patch_id {
        PatchId::Numeric(id) => {
            let id = id.to_be_bytes();
            let id = &id[count_leading_zeroes(&id)..];
            (BASE32HEX_NO_PADDING.encode(id), BASE64URL.encode(id))
        }
        PatchId::String(id) => (BASE32HEX_NO_PADDING.encode(id), BASE64URL.encode(id)),
    };

    // id64 might contain characters outside of the unreserved set which require percent encoding.
    // scan it and replace characters as needed. (ref: https://datatracker.ietf.org/doc/html/rfc6570#section-3.2.1)
    let mut id64_string_encoded: OutputBuffer = Default::default();
    for byte in id64_string.as_bytes() {
        let byte_info = BYTE_INFO_MAP[*byte as usize];
        match byte_info {
            ByteInfo::CopiedLiteralUnreserved | ByteInfo::CopiedLiteralHexDigit => {
                id64_string_encoded.append(*byte)
            }
            _ => id64_string_encoded.append_percent_encoded(*byte).unwrap(),
        }
    }

    expand_template_inner(template_string, &id_string, &id64_string_encoded.0)
}

const BASE32HEX_NO_PADDING: data_encoding::Encoding = new_encoding! {
    symbols: "0123456789ABCDEFGHIJKLMNOPQRSTUV",
};

fn count_leading_zeroes(id: &[u8]) -> usize {
    id.iter().take_while(|b| **b == 0).count().min(id.len() - 1)
}

/// Expands template string with the provided id and id64 string values.
fn expand_template_inner(
    template_string: &str,
    id_value: &str,
    id64_value: &str,
) -> Result<String, UriTemplateError> {
    let mut output: OutputBuffer = Default::default();

    // Parsing and expansion is implemented using a state machine (ParseState).
    // Each byte in the input template is first classified by BYTE_INFO_MAP and
    // then the classification is used as an input to the state machine transitions.
    //
    // At a high level expansion works like this
    // - Literals are checked if they are allowed and then either copied directly to
    //   the output or percent encoded if required.
    // - Otherwise if we are in an expression it attempts to match the variable name
    //   to one of the predefined variables. If a match is found the variables value
    //   is substituted.
    //
    // see: https://datatracker.ietf.org/doc/html/rfc6570#section-3 for more details.
    let mut state = ParseState::Literal;
    for byte in template_string.as_bytes() {
        let byte_info = BYTE_INFO_MAP[*byte as usize];
        state = match state {
            ParseState::Literal => output.handle_literal(byte_info, *byte)?,
            ParseState::LiteralPercentEncoded(digit) => {
                output.handle_percent_encoding(byte_info, digit, *byte)?
            }
            ParseState::Expression(variable) => {
                output.handle_expression(variable, *byte, id_value, id64_value)?
            }
        }
    }

    if !matches!(state, ParseState::Literal) {
        // Should always end back in the literal state otherwise we're in an incomplete expression
        // or percent encoding.
        return Err(UriTemplateError);
    }

    Ok(output.0)
}

impl OutputBuffer {
    /// Handles the next literal character.
    ///
    /// Either:
    /// - Copies literal as is into the output.
    /// - Percent encodes the character
    /// - Substitution expression begins.
    /// - Something invalid encountered.
    fn handle_literal(
        &mut self,
        byte_info: ByteInfo,
        value: u8,
    ) -> Result<ParseState, UriTemplateError> {
        match byte_info {
            ByteInfo::Invalid => Err(UriTemplateError),
            ByteInfo::Percent => {
                self.append(value);
                Ok(ParseState::LiteralPercentEncoded(Digit::One))
            }
            ByteInfo::StartExpression => Ok(ParseState::Expression(Variable::Begin)),
            ByteInfo::CopiedLiteral
            | ByteInfo::CopiedLiteralHexDigit
            | ByteInfo::CopiedLiteralUnreserved => {
                self.append(value);
                Ok(ParseState::Literal)
            }
            ByteInfo::PercentEncodedLiteral => self
                .append_percent_encoded(value)
                .map(|_| ParseState::Literal),
        }
    }

    /// Checks if percent encoding is valid.
    ///
    /// Copies to the output if it is.
    fn handle_percent_encoding(
        &mut self,
        byte_info: ByteInfo,
        digit: Digit,
        value: u8,
    ) -> Result<ParseState, UriTemplateError> {
        match byte_info {
            ByteInfo::CopiedLiteralHexDigit => {
                self.append(value);
                match digit {
                    Digit::One => Ok(ParseState::LiteralPercentEncoded(Digit::Two)),
                    Digit::Two => Ok(ParseState::Literal),
                }
            }
            _ => Err(UriTemplateError),
        }
    }

    /// Decode the variable name in the expression and substitute a value if needed.
    ///
    /// - Value is substituted if one of the defined variable names are encountered.
    /// - Otherwise returns an error, the IFT spec disallows undefined variable names.
    fn handle_expression(
        &mut self,
        variable: Variable,
        value: u8,
        id_value: &str,
        id64_value: &str,
    ) -> Result<ParseState, UriTemplateError> {
        match (variable, value) {
            // ### Variable matching ###
            (Variable::Begin, b'i') => Ok(ParseState::Expression(Variable::I)),
            (Variable::Begin, b'd') => Ok(ParseState::Expression(Variable::D)),
            (Variable::I, b'd') => Ok(ParseState::Expression(Variable::ID)),
            (Variable::ID, b'6') => Ok(ParseState::Expression(Variable::ID6)),
            (Variable::ID6, b'4') => Ok(ParseState::Expression(Variable::ID64)),
            (Variable::D, b'1') => Ok(ParseState::Expression(Variable::DX(1))),
            (Variable::D, b'2') => Ok(ParseState::Expression(Variable::DX(2))),
            (Variable::D, b'3') => Ok(ParseState::Expression(Variable::DX(3))),
            (Variable::D, b'4') => Ok(ParseState::Expression(Variable::DX(4))),

            // ### termination states ###
            (Variable::ID, b'}') => {
                self.append_str(id_value);
                Ok(ParseState::Literal)
            }
            (Variable::ID64, b'}') => {
                self.append_str(id64_value);
                Ok(ParseState::Literal)
            }
            (Variable::DX(digit), b'}') => {
                self.append_id_digit(id_value, digit);
                Ok(ParseState::Literal)
            }

            // Anything else that doesn't exactly match one of the defined variables is an error.
            _ => Err(UriTemplateError),
        }
    }

    /// Appends the expanded value of d1, d2, d3, or d4.
    ///
    /// See: <https://w3c.github.io/IFT/Overview.html#uri-templates>
    fn append_id_digit(&mut self, id_value: &str, digit: u8) {
        self.append(
            *id_value
                .len()
                .checked_sub(digit.into())
                .and_then(|index| id_value.as_bytes().get(index))
                .unwrap_or(&b'_'),
        )
    }

    // Appends a string to the output.
    fn append_str(&mut self, value: &str) {
        self.0.push_str(value)
    }

    // Appends a single byte to the output.
    fn append(&mut self, byte: u8) {
        self.0.push(byte.into());
    }

    // Appends the percent encoded representation (%XX) of a byte to the output.
    fn append_percent_encoded(&mut self, byte: u8) -> Result<(), UriTemplateError> {
        write!(&mut self.0, "%{:02X}", byte).map_err(|_| UriTemplateError)
    }
}

/// Classifies each byte value [0-255] into how it is handled by uri template expansion.
#[derive(Copy, Clone, Default)]
enum ByteInfo {
    #[default]
    Invalid, // This byte is not allowed in a URI template
    Percent,                 // The % character starts a percent encoding
    CopiedLiteral,           // This byte should be copied directly
    CopiedLiteralHexDigit,   // This byte should be copied directly and it's a valid hex digit.
    CopiedLiteralUnreserved, //  This byte should be copied directly and it's a unreserved character.
    PercentEncodedLiteral,   // This byte should be percent encoded and then copied.
    StartExpression,         // { starts an expression.
}

impl ByteInfo {
    const fn new(value: u8) -> Self {
        match value {
            b'{' => ByteInfo::StartExpression,
            b'%' => ByteInfo::Percent,
            _ => {
                if !Self::ascii_allowed_as_literal(value) {
                    ByteInfo::Invalid
                } else if Self::ascii_url_reserved_or_unreserved(value) {
                    if value.is_ascii_hexdigit() {
                        ByteInfo::CopiedLiteralHexDigit
                    } else if Self::ascii_url_unreserved(value) {
                        ByteInfo::CopiedLiteralUnreserved
                    } else {
                        ByteInfo::CopiedLiteral
                    }
                } else {
                    ByteInfo::PercentEncodedLiteral
                }
            }
        }
    }

    const fn ascii_allowed_as_literal(value: u8) -> bool {
        // See: https://datatracker.ietf.org/doc/html/rfc6570#section-2.1
        match value {
            0x21
            | 0x23..=0x24
            | 0x26
            | 0x28..=0x3B
            | 0x3D
            | 0x3F..=0x5B
            | 0x5D
            | 0x5F
            | 0x61..=0x7A
            | 0x7E => true,
            _ => value > 0x7F, // All non-ascii bytes are allowed
        }
    }

    const fn ascii_url_unreserved(value: u8) -> bool {
        // See: https://datatracker.ietf.org/doc/html/rfc6570#section-1.5
        value.is_ascii_alphanumeric() || matches!(value, b'-' | b'.' | b'_' | b'~')
    }

    const fn ascii_url_reserved_or_unreserved(value: u8) -> bool {
        // See: https://datatracker.ietf.org/doc/html/rfc6570#section-1.5
        Self::ascii_url_unreserved(value)
            || matches!(value, |b':'| b'/'
                | b'?'
                | b'#'
                | b'['
                | b']'
                | b'@'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'=')
    }
}

// This macro generates the byte info array at compile time.
const NUM_U8S: usize = u8::MAX as usize + 1;
macro_rules! generate_byte_info_array {
    () => {{
        const ARRAY: [ByteInfo; NUM_U8S] = {
            let mut info = [ByteInfo::Invalid; NUM_U8S];
            let mut i = 0;
            while i < NUM_U8S {
                info[i] = ByteInfo::new(i as u8);
                i += 1;
            }
            info
        };
        ARRAY
    }};
}

/// This maps each possiblue byte (u8) value to an enum which classifies how that value is handled during expansion.
static BYTE_INFO_MAP: [ByteInfo; NUM_U8S] = generate_byte_info_array!();

#[cfg(test)]
pub(crate) mod tests {
    use crate::patchmap::PatchId;
    use crate::uri_templates::UriTemplateError;

    use super::{expand_template, expand_template_inner};

    #[test]
    fn spec_examples() {
        // Tests of all IFT spec URI template examples from:
        // https://w3c.github.io/IFT/Overview.html#uri-templates
        assert_eq!(
            expand_template("//foo.bar/{id}", &PatchId::Numeric(123)),
            Ok("//foo.bar/FC".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{id}", &PatchId::Numeric(0)),
            Ok("//foo.bar/00".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{d1}/{d2}/{id}", &PatchId::Numeric(478)),
            Ok("//foo.bar/0/F/07F0".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{d1}/{d2}/{d3}/{id}", &PatchId::Numeric(123)),
            Ok("//foo.bar/C/F/_/FC".to_string())
        );

        assert_eq!(
            expand_template(
                "//foo.bar/{d1}/{d2}/{d3}/{id}",
                &PatchId::String(Vec::from_iter("baz".as_bytes().iter().copied()))
            ),
            Ok("//foo.bar/K/N/G/C9GNK".to_string())
        );

        assert_eq!(
            expand_template(
                "//foo.bar/{d1}/{d2}/{d3}/{id}",
                &PatchId::String(Vec::from_iter("z".as_bytes().iter().copied()))
            ),
            Ok("//foo.bar/8/F/_/F8".to_string())
        );

        assert_eq!(
            expand_template(
                "//foo.bar/{d1}/{d2}/{d3}/{id}",
                &PatchId::String(Vec::from_iter("àbc".as_bytes().iter().copied()))
            ),
            Ok("//foo.bar/O/O/4/OEG64OO".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{id64}", &PatchId::Numeric(14000000)),
            Ok("//foo.bar/1Z-A".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{id64}", &PatchId::Numeric(0)),
            Ok("//foo.bar/AA%3D%3D".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{id64}", &PatchId::Numeric(17000000)),
            Ok("//foo.bar/AQNmQA%3D%3D".to_string())
        );

        assert_eq!(
            expand_template(
                "//foo.bar/{id64}",
                &PatchId::String(Vec::from_iter("àbc".as_bytes().iter().copied()))
            ),
            Ok("//foo.bar/w6BiYw%3D%3D".to_string())
        );
    }

    #[test]
    fn copied_literals_only() {
        assert_eq!(
            expand_template_inner("foo/bar$", "abc", "def"),
            Ok("foo/bar$".to_string())
        );
    }

    #[test]
    fn percent_encoding_copied() {
        assert_eq!(
            expand_template_inner("%af%AF%09", "abc", "def"),
            Ok("%af%AF%09".to_string())
        );

        assert_eq!(
            expand_template_inner("foo/b%a8", "abc", "def"),
            Ok("foo/b%a8".to_string())
        );

        assert_eq!(
            expand_template_inner("foo/b%bFgr", "abc", "def"),
            Ok("foo/b%bFgr".to_string())
        );
    }

    #[test]
    fn percent_encodes_literals() {
        assert_eq!(
            expand_template_inner("foo/bàr", "abc", "def"),
            Ok("foo/b%C3%A0r".to_string())
        );
    }

    #[test]
    fn valid_expansions() {
        assert_eq!(
            expand_template_inner("{id}{id64}", "abc", "def"),
            Ok("abcdef".to_string())
        );

        assert_eq!(
            expand_template_inner("//foo.bar/{id}", "abc", "def"),
            Ok("//foo.bar/abc".to_string())
        );

        assert_eq!(
            expand_template_inner("//foo.bar/{id}/baz", "abc", "def"),
            Ok("//foo.bar/abc/baz".to_string())
        );

        assert_eq!(
            expand_template_inner("//foo.bar/{id64}", "abc", "def"),
            Ok("//foo.bar/def".to_string())
        );

        assert_eq!(
            expand_template_inner("//foo.bar/{id64}/baz", "abc", "def"),
            Ok("//foo.bar/def/baz".to_string())
        );

        assert_eq!(
            expand_template_inner("//foo.bar/{d1}/{d2}/{d3}/{id}", "FC", "def"),
            Ok("//foo.bar/C/F/_/FC".to_string())
        );

        assert_eq!(
            expand_template_inner("//foo.bar/{d1}/{d2}/{d3}/{d4}/{id}", "ABCD", "def"),
            Ok("//foo.bar/D/C/B/A/ABCD".to_string())
        );
    }

    #[test]
    fn undefined_variables() {
        assert_eq!(
            expand_template_inner("//foo.bar/{idd}/baz", "abc", "def"),
            Err(UriTemplateError),
        );

        assert_eq!(
            expand_template_inner("//foo.bar/{idid}/baz", "abc", "def"),
            Err(UriTemplateError),
        );

        assert_eq!(
            expand_template_inner("//foo.bar/{id_id}/baz", "abc", "def"),
            Err(UriTemplateError),
        );

        assert_eq!(
            expand_template_inner("//foo.bar/{_id}/baz", "abc", "def"),
            Err(UriTemplateError),
        );

        assert_eq!(
            expand_template_inner("//foo.bar/{7id}/baz", "abc", "def"),
            Err(UriTemplateError),
        );

        assert_eq!(
            expand_template_inner("//foo.bar/{Id}/baz", "abc", "def"),
            Err(UriTemplateError),
        );

        assert_eq!(
            expand_template_inner("//foo.bar/{d5}/baz", "abc", "def"),
            Err(UriTemplateError),
        );

        assert_eq!(
            expand_template_inner("//foo.bar/{id74}/{id}", "abc", "def"),
            Err(UriTemplateError),
        );

        assert_eq!(
            expand_template_inner("//foo.bar/{foo%ab}", "abc", "def"),
            Err(UriTemplateError),
        );

        assert_eq!(
            expand_template_inner("//foo.bar/{%ab}", "abc", "def"),
            Err(UriTemplateError),
        );
    }

    #[test]
    fn unterminated_expression() {
        assert_eq!(
            expand_template_inner("{id64", "abc", "def"),
            Err(UriTemplateError)
        );
    }

    #[test]
    fn unsupported_operator() {
        assert_eq!(
            expand_template_inner("{+id}", "abc", "def"),
            Err(UriTemplateError)
        );
        assert_eq!(
            expand_template_inner("{.id}", "abc", "def"),
            Err(UriTemplateError)
        );
        assert_eq!(
            expand_template_inner("{/id}", "abc", "def"),
            Err(UriTemplateError)
        );
        assert_eq!(
            expand_template_inner("{/}", "abc", "def"),
            Err(UriTemplateError)
        );
    }

    #[test]
    fn bad_variable_name() {
        assert_eq!(
            // Variable names must have at least one char
            expand_template_inner("{}", "abc", "def"),
            Err(UriTemplateError)
        );
        assert_eq!(
            // Variable names must have at least one char
            expand_template_inner("{}}", "abc", "def"),
            Err(UriTemplateError)
        );

        assert_eq!(
            expand_template_inner("{id}}", "abc", "def"), // double closing brace
            Err(UriTemplateError)
        );

        assert_eq!(
            expand_template_inner("{i+d}", "abc", "def"),
            Err(UriTemplateError)
        );
        assert_eq!(
            expand_template_inner("{i/d}", "abc", "def"),
            Err(UriTemplateError)
        );
        assert_eq!(
            expand_template_inner("{.}", "abc", "def"), // beginning '.'s are not allowed
            Err(UriTemplateError)
        );
        assert_eq!(
            expand_template_inner("{a.}", "abc", "def"), // trailing '.'s are not allowed
            Err(UriTemplateError)
        );
        assert_eq!(
            expand_template_inner("{id.}", "abc", "def"), // trailing '.'s are not allowed
            Err(UriTemplateError)
        );
        assert_eq!(
            expand_template_inner("{i..d}", "abc", "def"), // .. is not allowed
            Err(UriTemplateError)
        );

        assert_eq!(
            expand_template_inner("{id:1}", "abc", "def"), // ":" prefix operator not allowed.
            Err(UriTemplateError)
        );

        assert_eq!(
            // Multiple variables in an expression is not supported at level 1.
            expand_template_inner("{id,id64}", "abc", "def"),
            Err(UriTemplateError)
        );
    }

    #[test]
    fn bad_percent_encoding_in_variable_names() {
        assert_eq!(
            // Unterminated
            expand_template_inner("{%}", "abc", "def"),
            Err(UriTemplateError)
        );
        assert_eq!(
            // Unterminated
            expand_template_inner("{%A}", "abc", "def"),
            Err(UriTemplateError)
        );
        assert_eq!(
            // non hex digit
            expand_template_inner("{%AG}", "abc", "def"),
            Err(UriTemplateError)
        );
        assert_eq!(
            // non hex digit
            expand_template_inner("{id%GA}", "abc", "def"),
            Err(UriTemplateError)
        );
    }

    #[test]
    fn invalid_percent_encoding() {
        assert_eq!(
            expand_template_inner("foo/b%a/", "abc", "def"),
            Err(UriTemplateError)
        );

        assert_eq!(
            expand_template_inner("foo/b%a", "abc", "def"),
            Err(UriTemplateError)
        );

        assert_eq!(
            expand_template_inner("foo/b%a{id}", "abc", "def"),
            Err(UriTemplateError)
        );
    }

    #[test]
    fn unexpected_close_brace() {
        assert_eq!(
            expand_template_inner("foo/b}ar", "abc", "def"),
            Err(UriTemplateError)
        );
    }

    #[test]
    fn invalid_characters() {
        assert_eq!(
            expand_template_inner("foo/\"bar\"", "abc", "def"),
            Err(UriTemplateError)
        );

        assert_eq!(
            expand_template_inner("foo bar", "abc", "def"),
            Err(UriTemplateError)
        );

        let mut input: String = "foo".to_string();
        input.push(0x00 as char);
        assert_eq!(
            expand_template_inner(&input, "abc", "def"),
            Err(UriTemplateError)
        );

        let mut input: String = "foo".to_string();
        input.push(0x1F as char);
        assert_eq!(
            expand_template_inner(&input, "abc", "def"),
            Err(UriTemplateError)
        );
    }
}
