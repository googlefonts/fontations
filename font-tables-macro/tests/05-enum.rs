use font_types_macro::FontThing;
use toy_types::*;

#[derive(Clone, Debug, FontThing)]
pub struct Cmap4 {
    pub format: uint16,
    pub length: uint16,
    pub language: uint16,
    pub seg_count_x2: uint16,
    pub search_range: uint16,
    pub entry_selector: uint16,
    pub range_shift: uint16,
}

#[derive(Clone, Debug, FontThing)]
pub struct Cmap6 {
    /// Format number is set to 6.
    format: uint16,
    /// This is the length in bytes of the subtable.
    length: uint16,
    /// For requirements on use of the language field, see “Use of the
    /// language field in 'cmap' subtables” in this document.
    language: uint16,
    /// First character code of subrange.
    first_code: uint16,
    /// Number of character codes in subrange.
    entry_count: uint16,
}

#[derive(Clone, Debug, FontThing)]
#[font_thing(format(uint16))]
enum CmapSubtable {
    #[font_thing(format = 4)]
    Format4(Cmap4),
    #[font_thing(format = 6)]
    Format6(Cmap6),
}

fn main() {
}
