//! the [VARC (Variable Composite/Component)](https://github.com/harfbuzz/boring-expansion-spec/blob/main/VARC.md) table

use super::variations::PackedDeltas;
pub use super::{
    layout::{Condition, CoverageTable},
    postscript::Index2,
};

include!("../../generated/generated_varc.rs");

impl<'a> Varc<'a> {
    pub fn axis_indices(&self, nth: usize) -> Result<PackedDeltas, ReadError> {
        let Some(axis_indices_list) = self.axis_indices_list() else {
            return Err(ReadError::InvalidCollectionIndex(nth as u32));
        };
        let axis_indices_list = axis_indices_list?;
        let raw = axis_indices_list
            .get(nth)
            .map_err(|_| ReadError::OutOfBounds)?;
        Ok(PackedDeltas::new(raw.into()))
    }
}

#[cfg(test)]
mod tests {
    use crate::{FontRef, ReadError, TableProvider};

    use super::{Condition, Varc};

    impl Varc<'_> {
        fn conditions(&self) -> impl Iterator<Item = Condition<'_>> {
            self.condition_list()
                .expect("A condition list is present")
                .expect("We could read the condition list")
                .conditions()
                .iter()
                .enumerate()
                .map(|(i, c)| c.unwrap_or_else(|e| panic!("condition {i} {e}")))
        }

        fn axis_indices_count(&self) -> Result<usize, ReadError> {
            let Some(axis_indices_list) = self.axis_indices_list() else {
                return Ok(0);
            };
            let axis_indices_list = axis_indices_list?;
            Ok(axis_indices_list.count() as usize)
        }
    }

    #[test]
    fn read_cjk_0x6868() {
        let font = FontRef::new(font_test_data::varc::CJK_6868).unwrap();
        let table = font.varc().unwrap();
        table.coverage().unwrap(); // should have coverage
    }

    #[test]
    fn identify_all_conditional_types() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();

        // We should have all 5 condition types in order
        assert_eq!(
            (1..=5).collect::<Vec<_>>(),
            table.conditions().map(|c| c.format()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn read_condition_format1_axis_range() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();
        let Some(Condition::Format1AxisRange(condition)) =
            table.conditions().find(|c| c.format() == 1)
        else {
            panic!("No such item");
        };

        assert_eq!(
            (0, 0.5, 1.0),
            (
                condition.axis_index(),
                condition.filter_range_min_value().to_f32(),
                condition.filter_range_max_value().to_f32(),
            )
        );
    }

    #[test]
    fn read_condition_format2_variable_value() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();
        let Some(Condition::Format2VariableValue(condition)) =
            table.conditions().find(|c| c.format() == 2)
        else {
            panic!("No such item");
        };

        assert_eq!((1, 2), (condition.default_value(), condition.var_index(),));
    }

    #[test]
    fn read_condition_format3_and() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();
        let Some(Condition::Format3And(condition)) = table.conditions().find(|c| c.format() == 3)
        else {
            panic!("No such item");
        };

        // Should reference a format 1 and a format 2
        assert_eq!(
            vec![1, 2],
            condition
                .conditions()
                .iter()
                .map(|c| c.unwrap().format())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn read_condition_format4_or() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();
        let Some(Condition::Format4Or(condition)) = table.conditions().find(|c| c.format() == 4)
        else {
            panic!("No such item");
        };

        // Should reference a format 1 and a format 2
        assert_eq!(
            vec![1, 2],
            condition
                .conditions()
                .iter()
                .map(|c| c.unwrap().format())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn read_condition_format5_negate() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();
        let Some(Condition::Format5Negate(condition)) =
            table.conditions().find(|c| c.format() == 5)
        else {
            panic!("No such item");
        };

        // Should reference a format 1
        assert_eq!(1, condition.condition().unwrap().format(),);
    }

    #[test]
    fn read_axis_indices_list() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();
        assert_eq!(table.axis_indices_count().unwrap(), 2);
        assert_eq!(
            vec![2, 3, 4, 5, 6],
            table.axis_indices(1).unwrap().iter().collect::<Vec<_>>()
        );
    }
}
