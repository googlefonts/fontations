//! Implementation of the specific variant of URL template expansion required by the IFT specification.
//!
//! Context: <https://w3c.github.io/IFT/Overview.html#url-templates>
//!
//! URL templates in IFT use an series of one byte opcodes to insert literals or expand template variables.
use data_encoding::BASE64URL;
use data_encoding_macro::new_encoding;
use std::io::{Cursor, Read};

use crate::patchmap::PatchId;

/// Indicates a malformed URI template was encountered.
#[derive(Debug, PartialEq, Eq)]
pub enum UrlTemplateError {
    InvalidOpCode(u8),
    UnexpectedEndOfBuffer(u8),
    InvalidUtf8,
}

impl std::fmt::Display for UrlTemplateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            UrlTemplateError::InvalidOpCode(op_code) => write!(f, "Invalid URL template encountered. Unrecognized op code ({})", op_code),
            UrlTemplateError::UnexpectedEndOfBuffer(op_code) => write!(f, "Invalid URL template encountered. Unexpected end of buffer handling literal insertion (op_code = {})", op_code),
            UrlTemplateError::InvalidUtf8 => write!(f, "Invalid utf8 literal encountered in URL template."),
        }
    }
}

enum OpCode {
    InsertLiteral(usize),
    InsertVariable(Variable),
}

enum Variable {
    Id32,
    Digit(u8),
    Id64,
}

impl TryFrom<u8> for OpCode {
    type Error = UrlTemplateError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        // See: https://w3c.github.io/IFT/Overview.html#url-templates
        if value & (1 << 7) > 0 {
            Ok(OpCode::InsertVariable(value.try_into()?))
        } else if value != 0 {
            Ok(OpCode::InsertLiteral(value as usize))
        } else {
            Err(UrlTemplateError::InvalidOpCode(value))
        }
    }
}

impl TryFrom<u8> for Variable {
    type Error = UrlTemplateError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        // See: https://w3c.github.io/IFT/Overview.html#url-templates
        match value {
            128 => Ok(Variable::Id32),
            129 => Ok(Variable::Digit(1)),
            130 => Ok(Variable::Digit(2)),
            131 => Ok(Variable::Digit(3)),
            132 => Ok(Variable::Digit(4)),
            133 => Ok(Variable::Id64),
            _ => Err(UrlTemplateError::InvalidOpCode(value)),
        }
    }
}

struct State<'a> {
    template_bytes: Cursor<&'a [u8]>,
    output_buffer: String,
    id32_string: String,
    id64_string: String,
}

impl State<'_> {
    fn next_byte(&mut self) -> Option<u8> {
        let mut next_byte = [0u8];
        if self.template_bytes.read_exact(&mut next_byte).is_err() {
            return None;
        }
        Some(next_byte[0])
    }

    fn apply(&mut self, op_code: OpCode) -> Result<(), UrlTemplateError> {
        match op_code {
            OpCode::InsertLiteral(count) => self.copy_literals(count),
            OpCode::InsertVariable(variable) => {
                self.expand_variable(variable);
                Ok(())
            }
        }
    }

    fn copy_literals(&mut self, count: usize) -> Result<(), UrlTemplateError> {
        let mut literals = vec![0; count];
        self.template_bytes
            .read_exact(literals.as_mut_slice())
            .map_err(|_| UrlTemplateError::UnexpectedEndOfBuffer(count as u8))?;

        let literals = String::from_utf8(literals).map_err(|_| UrlTemplateError::InvalidUtf8)?;
        self.output_buffer.push_str(&literals);
        Ok(())
    }

    fn expand_variable(&mut self, variable: Variable) {
        match variable {
            Variable::Id32 => self.output_buffer.push_str(&self.id32_string),
            Variable::Digit(digit) => self
                .output_buffer
                .push(Self::id_digit(&self.id32_string, digit).into()),
            Variable::Id64 => self.output_buffer.push_str(&self.id64_string),
        };
    }

    fn id_digit(id_value: &str, digit: u8) -> u8 {
        id_value
            .len()
            .checked_sub(digit.into())
            .and_then(|index| id_value.as_bytes().get(index).copied())
            .unwrap_or(b'_')
    }
}

impl std::error::Error for UrlTemplateError {}

/// Implements url template expansion from incremental font transfer.
///
/// Specification: <https://w3c.github.io/IFT/Overview.html#url-templates>
pub(crate) fn expand_template(
    template_bytes: &[u8],
    patch_id: &PatchId,
) -> Result<String, UrlTemplateError> {
    let (id32_string, id64_string) = match &patch_id {
        PatchId::Numeric(id) => {
            let id = id.to_be_bytes();
            let id = &id[count_leading_zeroes(&id)..];
            (BASE32HEX_NO_PADDING.encode(id), BASE64URL.encode(id))
        }
        PatchId::String(id) => (BASE32HEX_NO_PADDING.encode(id), BASE64URL.encode(id)),
    };
    let id64_string = id64_string.replace("=", "%3D");

    let mut state = State {
        template_bytes: Cursor::new(template_bytes),
        output_buffer: Default::default(),
        id32_string,
        id64_string,
    };

    while let Some(next_byte) = state.next_byte() {
        state.apply(next_byte.try_into()?)?;
    }

    Ok(state.output_buffer)
}

const BASE32HEX_NO_PADDING: data_encoding::Encoding = new_encoding! {
    symbols: "0123456789ABCDEFGHIJKLMNOPQRSTUV",
};

fn count_leading_zeroes(id: &[u8]) -> usize {
    id.iter().take_while(|b| **b == 0).count().min(id.len() - 1)
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::patchmap::PatchId;
    use crate::url_templates::UrlTemplateError;

    use super::expand_template;

    fn check_numeric(bytes: &[u8], id: u32, expected: &str) {
        assert_eq!(
            expand_template(bytes, &PatchId::Numeric(id),),
            Ok(expected.to_string())
        );
    }

    fn check_string(bytes: &[u8], id: &[u8], expected: &str) {
        assert_eq!(
            expand_template(bytes, &PatchId::String(Vec::from(id)),),
            Ok(expected.to_string())
        );
    }

    fn check_error(bytes: &[u8], id: u32, expected: UrlTemplateError) {
        assert_eq!(
            expand_template(bytes, &PatchId::Numeric(id),),
            Err(expected)
        );
    }

    #[test]
    fn spec_examples() {
        // From https://w3c.github.io/IFT/Overview.html#url-templates
        check_numeric(
            &[
                16, b'h', b't', b't', b'p', b's', b':', b'/', b'/', b'f', b'o', b'o', b'.', b'b',
                b'a', b'r', b'/', 128,
            ],
            123,
            "https://foo.bar/FC",
        );
        check_numeric(
            &[8, b'f', b'o', b'o', b'?', b'b', b'a', b'r', b'=', 128],
            123,
            "foo?bar=FC",
        );
        check_numeric(
            &[
                10, b'/', b'/', b'f', b'o', b'o', b'.', b'b', b'a', b'r', b'/', 128,
            ],
            0,
            "//foo.bar/00",
        );
        check_numeric(
            &[
                5, b'/', b'f', b'o', b'o', b'/', 129, 1, b'/', 130, 1, b'/', 128,
            ],
            478,
            "/foo/0/F/07F0",
        );
        check_numeric(
            &[
                5, b'/', b'f', b'o', b'o', b'/', 129, 1, b'/', 130, 1, b'/', 131, 1, b'/', 128,
            ],
            123,
            "/foo/C/F/_/FC",
        );

        check_string(
            &[
                4, b'f', b'o', b'o', b'/', 129, 1, b'/', 130, 1, b'/', 131, 1, b'/', 128,
            ],
            b"baz",
            "foo/K/N/G/C9GNK",
        );

        check_string(
            &[
                4, b'f', b'o', b'o', b'/', 129, 1, b'/', 130, 1, b'/', 131, 1, b'/', 128,
            ],
            b"z",
            "foo/8/F/_/F8",
        );

        check_numeric(
            &[
                10, b'/', b'/', b'f', b'o', b'o', b'.', b'b', b'a', b'r', b'/', 133,
            ],
            14_000_000,
            "//foo.bar/1Z-A",
        );
        check_numeric(
            &[
                10, b'/', b'/', b'f', b'o', b'o', b'.', b'b', b'a', b'r', b'/', 133,
            ],
            0,
            "//foo.bar/AA%3D%3D",
        );
        check_numeric(
            &[
                10, b'/', b'/', b'f', b'o', b'o', b'.', b'b', b'a', b'r', b'/', 133,
            ],
            17_000_000,
            "//foo.bar/AQNmQA%3D%3D",
        );
        check_string(
            &[
                10, b'/', b'/', b'f', b'o', b'o', b'.', b'b', b'a', b'r', b'/', 133,
            ],
            &[0xc3, 0xa0, 0x62, 0x63],
            "//foo.bar/w6BiYw%3D%3D",
        );
    }

    #[test]
    fn spec_error_examples() {
        // From https://w3c.github.io/IFT/Overview.html#url-templates
        check_error(
            &[4, b'f', b'o', b'o', b'/', 150],
            123,
            UrlTemplateError::InvalidOpCode(150),
        );

        check_error(
            &[4, b'f', b'o', b'o', b'/', 0, 128],
            123,
            UrlTemplateError::InvalidOpCode(0),
        );

        check_error(
            &[10, b'f', b'o', b'o', b'/', 128],
            123,
            UrlTemplateError::UnexpectedEndOfBuffer(10),
        );

        check_error(
            &[4, b'f', b'o', b'o', 0x85, 128],
            123,
            UrlTemplateError::InvalidUtf8,
        );
    }

    #[test]
    fn empty_template() {
        check_numeric(&[], 123, "");
    }

    #[test]
    fn copied_literals_only() {
        check_numeric(
            &[7, b'f', b'o', b'o', b'0', b'b', b'a', b'r'],
            123,
            "foo0bar",
        );
    }

    #[test]
    fn expansion_only() {
        check_numeric(&[128, 133], 123, "FCew%3D%3D");
    }
}
