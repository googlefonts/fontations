//! impl subset() for maxp
use crate::{Plan, Subset, SubsetError, SubsetFlags};
use write_fonts::{
    read::{tables::maxp::Maxp, FontRef, TopLevelTable},
    types::Version16Dot16,
    FontBuilder,
};

// reference: subset() for maxp in harfbuzz
// https://github.com/harfbuzz/harfbuzz/blob/a070f9ebbe88dc71b248af9731dd49ec93f4e6e6/src/hb-ot-maxp-table.hh#L97
impl Subset for Maxp<'_> {
    fn subset(
        &self,
        plan: &Plan,
        _font: &FontRef,
        builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        let num_glyphs = plan.num_output_glyphs.min(0xFFFF) as u16;
        let mut out = self.offset_data().as_bytes().to_owned();
        out.get_mut(4..6)
            .unwrap()
            .copy_from_slice(&num_glyphs.to_be_bytes());

        //drop hints
        if self.version() == Version16Dot16::VERSION_1_0
            && plan
                .subset_flags
                .contains(SubsetFlags::SUBSET_FLAGS_NO_HINTING)
        {
            //maxZones
            out.get_mut(14..16).unwrap().copy_from_slice(&[0, 1]);
            //maxTwilightPoints..maxSizeOfInstructions
            out.get_mut(16..28).unwrap().fill(0);
        }
        builder.add_raw(Maxp::TAG, out);
        Ok(())
    }
}
