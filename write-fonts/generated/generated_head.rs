// THIS FILE IS AUTOGENERATED.
// Any changes to this file will be overwritten.
// For more information about how codegen works, see font-codegen/README.md

#[allow(unused_imports)]
use crate::codegen_prelude::*;

pub use read_fonts::tables::head::MacStyle;

impl FontWrite for MacStyle {
    fn write_into(&self, writer: &mut TableWriter) {
        writer.write_slice(&self.bits().to_be_bytes())
    }
}

/// The [head](https://docs.microsoft.com/en-us/typography/opentype/spec/head)
/// (font header) table.
#[derive(Clone, Debug)]
pub struct Head {
    /// Set by font manufacturer.
    pub font_revision: Fixed,
    /// To compute: set it to 0, sum the entire font as uint32, then
    /// store 0xB1B0AFBA - sum. If the font is used as a component in a
    /// font collection file, the value of this field will be
    /// invalidated by changes to the file structure and font table
    /// directory, and must be ignored.
    pub checksum_adjustment: u32,
    /// Set to 0x5F0F3CF5.
    pub magic_number: u32,
    /// See the flags enum
    pub flags: u16,
    /// Set to a value from 16 to 16384. Any value in this range is
    /// valid. In fonts that have TrueType outlines, a power of 2 is
    /// recommended as this allows performance optimizations in some
    /// rasterizers.
    pub units_per_em: u16,
    /// Number of seconds since 12:00 midnight that started January 1st
    /// 1904 in GMT/UTC time zone.
    pub created: LongDateTime,
    /// Number of seconds since 12:00 midnight that started January 1st
    /// 1904 in GMT/UTC time zone.
    pub modified: LongDateTime,
    /// Minimum x coordinate across all glyph bounding boxes.
    pub x_min: i16,
    /// Minimum y coordinate across all glyph bounding boxes.
    pub y_min: i16,
    /// Maximum x coordinate across all glyph bounding boxes.
    pub x_max: i16,
    /// Maximum y coordinate across all glyph bounding boxes.
    pub y_max: i16,
    /// see somewhere else
    pub mac_style: MacStyle,
    /// Smallest readable size in pixels.
    pub lowest_rec_ppem: u16,
    /// Deprecated (Set to 2).
    pub font_direction_hint: i16,
    /// 0 for short offsets (Offset16), 1 for long (Offset32).
    pub index_to_loc_format: i16,
}

impl Default for Head {
    fn default() -> Self {
        Self {
            font_revision: Default::default(),
            checksum_adjustment: Default::default(),
            magic_number: 0x5F0F3CF5,
            flags: Default::default(),
            units_per_em: Default::default(),
            created: Default::default(),
            modified: Default::default(),
            x_min: Default::default(),
            y_min: Default::default(),
            x_max: Default::default(),
            y_max: Default::default(),
            mac_style: Default::default(),
            lowest_rec_ppem: Default::default(),
            font_direction_hint: 2,
            index_to_loc_format: Default::default(),
        }
    }
}

impl Head {
    /// Construct a new `Head`
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        font_revision: Fixed,
        checksum_adjustment: u32,
        flags: u16,
        units_per_em: u16,
        created: LongDateTime,
        modified: LongDateTime,
        x_min: i16,
        y_min: i16,
        x_max: i16,
        y_max: i16,
        mac_style: MacStyle,
        lowest_rec_ppem: u16,
        index_to_loc_format: i16,
    ) -> Self {
        Self {
            font_revision,
            checksum_adjustment,
            flags,
            units_per_em,
            created,
            modified,
            x_min,
            y_min,
            x_max,
            y_max,
            mac_style,
            lowest_rec_ppem,
            index_to_loc_format,
            ..Default::default()
        }
    }
}

impl FontWrite for Head {
    #[allow(clippy::unnecessary_cast)]
    fn write_into(&self, writer: &mut TableWriter) {
        (MajorMinor::VERSION_1_0 as MajorMinor).write_into(writer);
        self.font_revision.write_into(writer);
        self.checksum_adjustment.write_into(writer);
        self.magic_number.write_into(writer);
        self.flags.write_into(writer);
        self.units_per_em.write_into(writer);
        self.created.write_into(writer);
        self.modified.write_into(writer);
        self.x_min.write_into(writer);
        self.y_min.write_into(writer);
        self.x_max.write_into(writer);
        self.y_max.write_into(writer);
        self.mac_style.write_into(writer);
        self.lowest_rec_ppem.write_into(writer);
        self.font_direction_hint.write_into(writer);
        self.index_to_loc_format.write_into(writer);
        (0 as i16).write_into(writer);
    }
    fn name(&self) -> &'static str {
        "Head"
    }
}

impl Validate for Head {
    fn validate_impl(&self, _ctx: &mut ValidationCtx) {}
}

impl TopLevelTable for Head {
    const TAG: Tag = Tag::new(b"head");
}

impl<'a> FromObjRef<read_fonts::tables::head::Head<'a>> for Head {
    fn from_obj_ref(obj: &read_fonts::tables::head::Head<'a>, _: FontData) -> Self {
        Head {
            font_revision: obj.font_revision(),
            checksum_adjustment: obj.checksum_adjustment(),
            magic_number: obj.magic_number(),
            flags: obj.flags(),
            units_per_em: obj.units_per_em(),
            created: obj.created(),
            modified: obj.modified(),
            x_min: obj.x_min(),
            y_min: obj.y_min(),
            x_max: obj.x_max(),
            y_max: obj.y_max(),
            mac_style: obj.mac_style(),
            lowest_rec_ppem: obj.lowest_rec_ppem(),
            font_direction_hint: obj.font_direction_hint(),
            index_to_loc_format: obj.index_to_loc_format(),
        }
    }
}

impl<'a> FromTableRef<read_fonts::tables::head::Head<'a>> for Head {}

impl<'a> FontRead<'a> for Head {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        <read_fonts::tables::head::Head as FontRead>::read(data).map(|x| x.to_owned_table())
    }
}
