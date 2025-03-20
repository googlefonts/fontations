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
use std::fmt::Write;
use std::sync::OnceLock;

enum ParseState {
    // Literal parsing
    Literal,
    LiteralPercentEncoded(Digit),

    // Expression parsing,
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
    Undefined,
    Dot,
    PercentEncoding(Digit),
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
pub struct UriTemplateError; // TODO(garretrieger): change patchmap to use this.

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
///
/// All arguments are assumed to be utf8 encoded strings.
pub(crate) fn expand_template(
    template_string: &str,
    id_value: &str,
    id64_value: &str,
) -> Result<String, UriTemplateError> {
    // TODO(garretrieger): additional method which take id as the raw integer or id string and convert to id and id64 as needed.
    let mut output: OutputBuffer = Default::default();

    let mut state = ParseState::Literal;
    let byte_info_map = byte_info_map();

    for byte in template_string.as_bytes() {
        let byte_info = &byte_info_map[*byte as usize];
        state = match state {
            ParseState::Literal => output.handle_literal(byte_info)?,
            ParseState::LiteralPercentEncoded(digit) => {
                output.handle_percent_encoding(byte_info, digit)?
            }
            ParseState::Expression(variable) => {
                output.handle_expression(byte_info, variable, id_value, id64_value)?
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
    fn handle_literal(&mut self, byte_info: &ByteInfo) -> Result<ParseState, UriTemplateError> {
        match byte_info.literal_class {
            LiteralClass::Invalid => Err(UriTemplateError),
            LiteralClass::Percent => {
                self.append(byte_info.value);
                Ok(ParseState::LiteralPercentEncoded(Digit::One))
            }
            LiteralClass::StartExpression => Ok(ParseState::Expression(Variable::Begin)),
            LiteralClass::LiteralCopied => {
                self.append(byte_info.value);
                Ok(ParseState::Literal)
            }
            LiteralClass::LiteralPercentEncoded => self
                .append_percent_encoded(byte_info.value)
                .map(|_| ParseState::Literal),
        }
    }

    /// Checks if percent encoding is valid.
    ///
    /// Copies to the output if it is.
    fn handle_percent_encoding(
        &mut self,
        byte_info: &ByteInfo,
        digit: Digit,
    ) -> Result<ParseState, UriTemplateError> {
        if !byte_info.is_hex_digit {
            return Err(UriTemplateError);
        }
        self.append(byte_info.value);
        match digit {
            Digit::One => Ok(ParseState::LiteralPercentEncoded(Digit::Two)),
            Digit::Two => Ok(ParseState::Literal),
        }
    }

    /// Decode the variable name in the expression and substitute a value if needed.
    ///
    /// - Value is substituted if one of the defined variable names are encountered.
    /// - Otherwise the variable name is undefined and the expression is replaced with an empty string.
    /// - Also validates the variable name follows level 1 expression grammar (<https://datatracker.ietf.org/doc/html/rfc6570#section-2.2>)
    ///   and returns an error if it doesn't.
    fn handle_expression(
        &mut self,
        byte_info: &ByteInfo,
        variable: Variable,
        id_value: &str,
        id64_value: &str,
    ) -> Result<ParseState, UriTemplateError> {
        if !byte_info.is_varchar {
            return Err(UriTemplateError);
        }

        match (variable, byte_info.value) {
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
                // TODO percent encode any characters in id64 as needed.
                self.append_str(id64_value);
                Ok(ParseState::Literal)
            }
            (Variable::DX(digit), b'}') => {
                self.append_id_digit(id_value, digit);
                Ok(ParseState::Literal)
            }
            (Variable::Undefined, b'}') => {
                // Undefined variable name just ignore it.
                Ok(ParseState::Literal)
            }
            (Variable::PercentEncoding(_), b'}') => Err(UriTemplateError), // Unterminated percent encoding

            // ### Percent Encoding Validation ###
            (Variable::PercentEncoding(digit), _) => {
                if !byte_info.is_hex_digit {
                    return Err(UriTemplateError);
                }

                match digit {
                    Digit::One => Ok(ParseState::Expression(Variable::PercentEncoding(
                        Digit::Two,
                    ))),
                    Digit::Two => Ok(ParseState::Expression(Variable::Undefined)),
                }
            }

            // ### Dot validity checking ###
            (Variable::Begin, b'.') => Err(UriTemplateError), // . operator not allowed.
            (Variable::Dot, b'}') | (Variable::Dot, b'.') => {
                Err(UriTemplateError) // trailing . or .. is not allowed.
            }
            (_, b'.') => Ok(ParseState::Expression(Variable::Dot)),

            // ### Enter percent encoding ###
            (_, b'%') => Ok(ParseState::Expression(Variable::PercentEncoding(
                Digit::One,
            ))),

            // ### Everything else ###
            (_, b'}') => Err(UriTemplateError), // Unexpected closing brace

            // Just skipping through an undefined variable name.
            _ => Ok(ParseState::Expression(Variable::Undefined)),
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

/// Stores information on how the template expansion treats each possible byte
/// value.
#[derive(Copy, Clone)]
struct ByteInfo {
    literal_class: LiteralClass,
    is_hex_digit: bool,
    is_varchar: bool,
    value: u8,
}

/// Classifies each byte value [0-255] into how it is handled by literal expansion.
#[derive(Copy, Clone)]
enum LiteralClass {
    Invalid,               // This byte is not allowed in a URI template
    Percent,               // The % character starts a percent encoding
    LiteralCopied,         // This byte should be copied directly
    LiteralPercentEncoded, // This byte should be percent encoded and then copied.
    StartExpression,       // { starts an expression.
}

impl Default for ByteInfo {
    fn default() -> Self {
        ByteInfo {
            literal_class: LiteralClass::Invalid,
            is_hex_digit: false,
            is_varchar: false,
            value: 0,
        }
    }
}

impl ByteInfo {
    fn new(value: u8) -> Self {
        ByteInfo {
            literal_class: Self::literal_class(value),
            is_hex_digit: value.is_ascii_hexdigit(),
            is_varchar: Self::is_varchar(value),
            value,
        }
    }

    /// Returns true if byte is a varchar.
    ///
    /// As defined here: <https://datatracker.ietf.org/doc/html/rfc6570#section-2.3>
    fn is_varchar(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'_' || byte == b'%' || byte == b'}'
    }

    fn ascii_allowed_as_literal(value: u8) -> bool {
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

    fn ascii_url_reserved_or_unreserved(value: u8) -> bool {
        // See: https://datatracker.ietf.org/doc/html/rfc6570#section-1.5
        if value.is_ascii_alphanumeric() {
            return true;
        }

        matches!(
            value,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b':'
                | b'/'
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
                | b'='
        )
    }

    fn literal_class(value: u8) -> LiteralClass {
        match value {
            b'{' => LiteralClass::StartExpression,
            b'%' => LiteralClass::Percent,
            _ => {
                if !Self::ascii_allowed_as_literal(value) {
                    LiteralClass::Invalid
                } else if Self::ascii_url_reserved_or_unreserved(value) {
                    LiteralClass::LiteralCopied
                } else {
                    LiteralClass::LiteralPercentEncoded
                }
            }
        }
    }
}

const NUM_U8S: usize = u8::MAX as usize + 1;

static BYTE_INFO_MAP: OnceLock<[ByteInfo; NUM_U8S]> = OnceLock::new();

/// Returns a map of information about each possible u8 byte value.
///
/// See ByteInfo for more details.
fn byte_info_map() -> &'static [ByteInfo; NUM_U8S] {
    // See: https://datatracker.ietf.org/doc/html/rfc6570#section-2.1
    BYTE_INFO_MAP.get_or_init(|| {
        let mut info: [ByteInfo; NUM_U8S] = [ByteInfo::default(); NUM_U8S];
        for value in 0..=u8::MAX {
            info[value as usize] = ByteInfo::new(value);
        }
        info
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::uri_templates::UriTemplateError;

    use super::expand_template;

    #[test]
    fn copied_literals_only() {
        assert_eq!(
            expand_template("foo/bar$", "abc", "def"),
            Ok("foo/bar$".to_string())
        );
    }

    #[test]
    fn percent_encoding_copied() {
        assert_eq!(
            expand_template("%af%AF%09", "abc", "def"),
            Ok("%af%AF%09".to_string())
        );

        assert_eq!(
            expand_template("foo/b%a8", "abc", "def"),
            Ok("foo/b%a8".to_string())
        );

        assert_eq!(
            expand_template("foo/b%bFgr", "abc", "def"),
            Ok("foo/b%bFgr".to_string())
        );
    }

    #[test]
    fn percent_encodes_literals() {
        assert_eq!(
            expand_template("foo/bàr", "abc", "def"),
            Ok("foo/b%C3%A0r".to_string())
        );
    }

    #[test]
    fn valid_expansions() {
        assert_eq!(
            expand_template("{id}{id64}", "abc", "def"),
            Ok("abcdef".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{id}", "abc", "def"),
            Ok("//foo.bar/abc".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{id}/baz", "abc", "def"),
            Ok("//foo.bar/abc/baz".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{id64}", "abc", "def"),
            Ok("//foo.bar/def".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{id64}/baz", "abc", "def"),
            Ok("//foo.bar/def/baz".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{d1}/{d2}/{d3}/{id}", "FC", "def"),
            Ok("//foo.bar/C/F/_/FC".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{d1}/{d2}/{d3}/{d4}/{id}", "ABCD", "def"),
            Ok("//foo.bar/D/C/B/A/ABCD".to_string())
        );
    }

    #[test]
    fn undefined_expansions() {
        assert_eq!(
            expand_template("//foo.bar/{idd}/baz", "abc", "def"),
            Ok("//foo.bar//baz".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{idid}/baz", "abc", "def"),
            Ok("//foo.bar//baz".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{id_id}/baz", "abc", "def"),
            Ok("//foo.bar//baz".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{_id}/baz", "abc", "def"),
            Ok("//foo.bar//baz".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{7id}/baz", "abc", "def"),
            Ok("//foo.bar//baz".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{Id}/baz", "abc", "def"),
            Ok("//foo.bar//baz".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{d5}/baz", "abc", "def"),
            Ok("//foo.bar//baz".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{id74}/{id}", "abc", "def"),
            Ok("//foo.bar//abc".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{foo_bar}", "abc", "def"),
            Ok("//foo.bar/".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{foo%ab}", "abc", "def"),
            Ok("//foo.bar/".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{%ab}", "abc", "def"),
            Ok("//foo.bar/".to_string())
        );
        assert_eq!(
            expand_template("//foo.bar/{%abg}", "abc", "def"),
            Ok("//foo.bar/".to_string())
        );

        assert_eq!(
            expand_template("//foo.bar/{foo.a.b}", "abc", "def"),
            Ok("//foo.bar/".to_string())
        );
    }

    #[test]
    fn unterminated_expression() {
        assert_eq!(
            expand_template("{id64", "abc", "def"),
            Err(UriTemplateError)
        );
    }

    #[test]
    fn unsupported_operator() {
        assert_eq!(
            expand_template("{+id}", "abc", "def"),
            Err(UriTemplateError)
        );
        assert_eq!(
            expand_template("{.id}", "abc", "def"),
            Err(UriTemplateError)
        );
        assert_eq!(
            expand_template("{/id}", "abc", "def"),
            Err(UriTemplateError)
        );
        assert_eq!(expand_template("{/}", "abc", "def"), Err(UriTemplateError));
    }

    #[test]
    fn bad_variable_name() {
        assert_eq!(
            // Variable names must have at least one char
            expand_template("{}", "abc", "def"),
            Err(UriTemplateError)
        );
        assert_eq!(
            // Variable names must have at least one char
            expand_template("{}}", "abc", "def"),
            Err(UriTemplateError)
        );

        assert_eq!(
            expand_template("{id}}", "abc", "def"), // double closing brace
            Err(UriTemplateError)
        );

        assert_eq!(
            expand_template("{i+d}", "abc", "def"),
            Err(UriTemplateError)
        );
        assert_eq!(
            expand_template("{i/d}", "abc", "def"),
            Err(UriTemplateError)
        );
        assert_eq!(
            expand_template("{.}", "abc", "def"), // beginning '.'s are not allowed
            Err(UriTemplateError)
        );
        assert_eq!(
            expand_template("{a.}", "abc", "def"), // trailing '.'s are not allowed
            Err(UriTemplateError)
        );
        assert_eq!(
            expand_template("{id.}", "abc", "def"), // trailing '.'s are not allowed
            Err(UriTemplateError)
        );
        assert_eq!(
            expand_template("{i..d}", "abc", "def"), // .. is not allowed
            Err(UriTemplateError)
        );

        assert_eq!(
            expand_template("{id:1}", "abc", "def"), // ":" prefix operator not allowed.
            Err(UriTemplateError)
        );

        assert_eq!(
            // Multiple variables in an expression is not supported at level 1.
            expand_template("{id,id64}", "abc", "def"),
            Err(UriTemplateError)
        );
    }

    #[test]
    fn bad_percent_encoding_in_variable_names() {
        assert_eq!(
            // Unterminated
            expand_template("{%}", "abc", "def"),
            Err(UriTemplateError)
        );
        assert_eq!(
            // Unterminated
            expand_template("{%A}", "abc", "def"),
            Err(UriTemplateError)
        );
        assert_eq!(
            // non hex digit
            expand_template("{%AG}", "abc", "def"),
            Err(UriTemplateError)
        );
        assert_eq!(
            // non hex digit
            expand_template("{id%GA}", "abc", "def"),
            Err(UriTemplateError)
        );
    }

    #[test]
    fn invalid_percent_encoding() {
        assert_eq!(
            expand_template("foo/b%a/", "abc", "def"),
            Err(UriTemplateError)
        );

        assert_eq!(
            expand_template("foo/b%a", "abc", "def"),
            Err(UriTemplateError)
        );

        assert_eq!(
            expand_template("foo/b%a{id}", "abc", "def"),
            Err(UriTemplateError)
        );
    }

    #[test]
    fn unexpected_close_brace() {
        assert_eq!(
            expand_template("foo/b}ar", "abc", "def"),
            Err(UriTemplateError)
        );
    }

    #[test]
    fn invalid_characters() {
        assert_eq!(
            expand_template("foo/\"bar\"", "abc", "def"),
            Err(UriTemplateError)
        );

        assert_eq!(
            expand_template("foo bar", "abc", "def"),
            Err(UriTemplateError)
        );

        let mut input: String = "foo".to_string();
        input.push(0x00 as char);
        assert_eq!(expand_template(&input, "abc", "def"), Err(UriTemplateError));

        let mut input: String = "foo".to_string();
        input.push(0x1F as char);
        assert_eq!(expand_template(&input, "abc", "def"), Err(UriTemplateError));
    }

    // TODO(garretrieger): add tests for valid cases
    // - variable expansion needs percent encoding.
}
