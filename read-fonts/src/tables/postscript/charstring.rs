//! Parsing for PostScript charstrings.

use super::{Error, Number};
use crate::{types::Fixed, Cursor};

/// PostScript charstring operator.
///
/// See <https://learn.microsoft.com/en-us/typography/opentype/spec/cff2charstr#appendix-a-cff2-charstring-command-codes>
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Operator {
    HStem,
    VStem,
    VMoveTo,
    RLineTo,
    HLineTo,
    VLineTo,
    RrCurveTo,
    CallSubr,
    Return,
    EndChar,
    VariationStoreIndex,
    Blend,
    HStemHm,
    HintMask,
    CntrMask,
    RMoveTo,
    HMoveTo,
    VStemHm,
    RCurveLine,
    RLineCurve,
    VvCurveTo,
    HhCurveTo,
    CallGsubr,
    VhCurveTo,
    HvCurveTo,
    HFlex,
    Flex,
    HFlex1,
    Flex1,
}

impl Operator {
    /// Creates an operator from the given opcode.
    fn from_opcode(opcode: u8) -> Option<Self> {
        use Operator::*;
        Some(match opcode {
            1 => HStem,
            3 => VStem,
            4 => VMoveTo,
            5 => RLineTo,
            6 => HLineTo,
            7 => VLineTo,
            8 => RrCurveTo,
            10 => CallSubr,
            11 => Return,
            14 => EndChar,
            15 => VariationStoreIndex,
            16 => Blend,
            18 => HStemHm,
            19 => HintMask,
            20 => CntrMask,
            21 => RMoveTo,
            22 => HMoveTo,
            23 => VStemHm,
            24 => RCurveLine,
            25 => RLineCurve,
            26 => VvCurveTo,
            27 => HhCurveTo,
            29 => CallGsubr,
            30 => VhCurveTo,
            31 => HvCurveTo,
            _ => return None,
        })
    }

    /// Creates an operator from the given extended opcode.
    ///
    /// These are preceded by a byte containing the escape value of 12.
    pub fn from_extended_opcode(opcode: u8) -> Option<Self> {
        use Operator::*;
        Some(match opcode {
            34 => HFlex,
            35 => Flex,
            36 => HFlex1,
            37 => Flex1,
            _ => return None,
        })
    }
}

/// Either a PostScript charstring operator or a (numeric) operand.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RawCommand {
    Operator(Operator),
    Operand(Number),
}

impl From<Operator> for RawCommand {
    fn from(value: Operator) -> Self {
        Self::Operator(value)
    }
}

impl<T> From<T> for RawCommand
where
    T: Into<Number>,
{
    fn from(value: T) -> Self {
        Self::Operand(value.into())
    }
}

/// Given a byte slice containing charstring data, returns an iterator yielding
/// raw operands and operators.
///
/// This does not perform any evaluation on the charstring.
pub fn raw_commands(
    charstring_data: &[u8],
) -> impl Iterator<Item = Result<RawCommand, Error>> + '_ + Clone {
    let mut cursor = crate::FontData::new(charstring_data).cursor();
    std::iter::from_fn(move || {
        if cursor.remaining_bytes() == 0 {
            None
        } else {
            Some(parse_raw_command(&mut cursor))
        }
    })
}

fn parse_raw_command(cursor: &mut Cursor) -> Result<RawCommand, Error> {
    // Escape opcode for accessing extensions.
    const ESCAPE: u8 = 12;
    let b0 = cursor.read::<u8>()?;
    Ok(if b0 == ESCAPE {
        let b1 = cursor.read::<u8>()?;
        RawCommand::Operator(
            Operator::from_extended_opcode(b1).ok_or(Error::InvalidCharstringOperator(b1))?,
        )
    } else {
        // See <https://learn.microsoft.com/en-us/typography/opentype/spec/cff2#table-3-operand-encoding>
        match b0 {
            28 | 29 | 32..=254 => super::dict::parse_int(cursor, b0 as i32)?.into(),
            255 => Fixed::from_bits(cursor.read::<i32>()?).into(),
            _ => RawCommand::Operator(
                Operator::from_opcode(b0).ok_or(Error::InvalidCharstringOperator(b0))?,
            ),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_charstring() {
        let charstring = &font_test_data::cff2::EXAMPLE[0x42..=0x43];
        let commands: Vec<_> = raw_commands(charstring)
            .map(|command| command.unwrap())
            .collect();
        // -107 callsubr
        let expected: &[RawCommand] = &[(-107).into(), Operator::CallSubr.into()];
        assert_eq!(commands, expected);
    }

    #[test]
    fn example_subr_charstring() {
        use Operator::*;
        let charstring = &font_test_data::cff2::EXAMPLE[0xc8..=0xe1];
        let commands: Vec<_> = raw_commands(charstring)
            .map(|command| command.unwrap())
            .collect();
        // 50 50 100 1 blend
        // 0 rmoveto
        // 500 -100 -200 1 blend
        // hlineto
        // 500 vlineto
        // -500 100 200 1 blend
        // hlineto
        let expected: &[RawCommand] = &[
            50.into(),
            50.into(),
            100.into(),
            1.into(),
            Blend.into(),
            0.into(),
            RMoveTo.into(),
            500.into(),
            (-100).into(),
            (-200).into(),
            1.into(),
            Blend.into(),
            HLineTo.into(),
            500.into(),
            VLineTo.into(),
            (-500).into(),
            100.into(),
            200.into(),
            1.into(),
            Blend.into(),
            HLineTo.into(),
        ];
        assert_eq!(commands, expected);
    }
}
