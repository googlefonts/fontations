//! Read condition trees without losing null offsets or truncated arrays.
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{tables::layout::Condition as ReadCondition, FontData, FontRead, ReadError},
    tables::layout::Condition,
};

/// Null condition offsets evaluate to true. Materialize a valid constant
/// when copying into write-fonts' non-nullable condition representation.
pub(crate) fn at_offset(
    data: FontData,
    offset: u32,
    depth: usize,
    budget: &mut usize,
) -> Result<Condition, ReadError> {
    if offset == 0 {
        if depth == 0 || *budget == 0 {
            return Err(ReadError::MalformedData("condition tree exceeds limits"));
        }
        *budget -= 1;
        return Ok(Condition::format_2_variable_value(1, u32::MAX));
    }
    own(
        FontData::new(
            data.as_bytes()
                .get(offset as usize..)
                .ok_or(ReadError::OutOfBounds)?,
        ),
        depth,
        budget,
    )
}

pub(crate) fn own(
    data: FontData,
    depth: usize,
    budget: &mut usize,
) -> Result<Condition, ReadError> {
    if depth == 0 || *budget == 0 {
        return Err(ReadError::MalformedData("condition tree exceeds limits"));
    }
    *budget -= 1;
    // Format zero is invalid in font data. Null offsets are handled above,
    // before attempting to read a condition at any address.
    match ReadCondition::read(data)? {
        ReadCondition::Format1AxisRange(c) => Ok(Condition::Format1AxisRange(c.to_owned_table())),
        ReadCondition::Format2VariableValue(c) => Ok(Condition::format_2_variable_value(
            c.default_value(),
            c.var_index(),
        )),
        ReadCondition::Format3And(c) => {
            if c.condition_offsets().len() != c.condition_count() as usize {
                return Err(ReadError::OutOfBounds);
            }
            let children = c
                .condition_offsets()
                .iter()
                .map(|o| at_offset(data, o.get().to_u32(), depth - 1, budget))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Condition::format_3_and(c.condition_count(), children))
        }
        ReadCondition::Format4Or(c) => {
            if c.condition_offsets().len() != c.condition_count() as usize {
                return Err(ReadError::OutOfBounds);
            }
            let children = c
                .condition_offsets()
                .iter()
                .map(|o| at_offset(data, o.get().to_u32(), depth - 1, budget))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Condition::format_4_or(c.condition_count(), children))
        }
        ReadCondition::Format5Negate(c) => Ok(Condition::format_5_negate(at_offset(
            data,
            c.condition_offset().to_u32(),
            depth - 1,
            budget,
        )?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_offsets_are_true_at_every_level() {
        let always = Condition::format_2_variable_value(1, u32::MAX);
        assert_eq!(
            at_offset(FontData::new(&[]), 0, 33, &mut 100).unwrap(),
            always
        );
        for (bytes, expected) in [
            (
                vec![0, 3, 1, 0, 0, 0],
                Condition::format_3_and(1, vec![always.clone()]),
            ),
            (
                vec![0, 4, 1, 0, 0, 0],
                Condition::format_4_or(1, vec![always.clone()]),
            ),
            (vec![0, 5, 0, 0, 0], Condition::format_5_negate(always)),
        ] {
            assert_eq!(own(FontData::new(&bytes), 33, &mut 100).unwrap(), expected);
        }
    }

    #[test]
    fn invalid_formats_truncated_arrays_and_recursive_offsets_are_rejected() {
        for bytes in [
            vec![0, 0],
            vec![0, 6],
            vec![0, 3, 2, 0, 0, 0],
            vec![0, 4, 2, 0, 0, 0],
            vec![0, 5, 0, 0],
            vec![0, 5, 0, 0, 2],
        ] {
            assert!(
                own(FontData::new(&bytes), 33, &mut 100).is_err(),
                "{bytes:?}"
            );
        }
        assert!(own(FontData::new(&[0, 5, 0, 0, 0]), 0, &mut 100).is_err());
        assert!(own(FontData::new(&[0, 5, 0, 0, 0]), 33, &mut 0).is_err());
    }
}
