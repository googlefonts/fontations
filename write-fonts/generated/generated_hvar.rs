// THIS FILE IS AUTOGENERATED.
// Any changes to this file will be overwritten.
// For more information about how codegen works, see font-codegen/README.md

#[allow(unused_imports)]
use crate::codegen_prelude::*;

/// The [HVAR (Horizontal Metrics Variations)](https://docs.microsoft.com/en-us/typography/opentype/spec/hvar) table
#[derive(Clone, Debug, Default)]
pub struct Hvar {
    /// Major version number of the horizontal metrics variations table — set to 1.
    /// Minor version number of the horizontal metrics variations table — set to 0.
    pub version: MajorMinor,
    /// Offset in bytes from the start of this table to the item variation store table.
    pub item_variation_store: OffsetMarker<ItemVariationStore, WIDTH_32>,
    /// Offset in bytes from the start of this table to the delta-set index mapping for advance widths (may be NULL).
    pub advance_width_mapping: NullableOffsetMarker<DeltaSetIndexMap, WIDTH_32>,
    /// Offset in bytes from the start of this table to the delta-set index mapping for left side bearings (may be NULL).
    pub lsb_mapping: NullableOffsetMarker<DeltaSetIndexMap, WIDTH_32>,
    /// Offset in bytes from the start of this table to the delta-set index mapping for right side bearings (may be NULL).
    pub rsb_mapping: NullableOffsetMarker<DeltaSetIndexMap, WIDTH_32>,
}

impl Hvar {
    /// Construct a new `Hvar`
    pub fn new(
        version: MajorMinor,
        item_variation_store: ItemVariationStore,
        advance_width_mapping: Option<DeltaSetIndexMap>,
        lsb_mapping: Option<DeltaSetIndexMap>,
        rsb_mapping: Option<DeltaSetIndexMap>,
    ) -> Self {
        Self {
            version,
            item_variation_store: item_variation_store.into(),
            advance_width_mapping: advance_width_mapping.into(),
            lsb_mapping: lsb_mapping.into(),
            rsb_mapping: rsb_mapping.into(),
        }
    }
}

impl FontWrite for Hvar {
    fn write_into(&self, writer: &mut TableWriter) {
        self.version.write_into(writer);
        self.item_variation_store.write_into(writer);
        self.advance_width_mapping.write_into(writer);
        self.lsb_mapping.write_into(writer);
        self.rsb_mapping.write_into(writer);
    }
    fn table_type(&self) -> TableType {
        TableType::TopLevel(Hvar::TAG)
    }
}

impl Validate for Hvar {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        ctx.in_table("Hvar", |ctx| {
            ctx.in_field("item_variation_store", |ctx| {
                self.item_variation_store.validate_impl(ctx);
            });
            ctx.in_field("advance_width_mapping", |ctx| {
                self.advance_width_mapping.validate_impl(ctx);
            });
            ctx.in_field("lsb_mapping", |ctx| {
                self.lsb_mapping.validate_impl(ctx);
            });
            ctx.in_field("rsb_mapping", |ctx| {
                self.rsb_mapping.validate_impl(ctx);
            });
        })
    }
}

impl TopLevelTable for Hvar {
    const TAG: Tag = Tag::new(b"HVAR");
}

impl<'a> FromObjRef<read_fonts::tables::hvar::Hvar<'a>> for Hvar {
    fn from_obj_ref(obj: &read_fonts::tables::hvar::Hvar<'a>, _: FontData) -> Self {
        Hvar {
            version: obj.version(),
            item_variation_store: obj.item_variation_store().to_owned_table(),
            advance_width_mapping: obj.advance_width_mapping().to_owned_table(),
            lsb_mapping: obj.lsb_mapping().to_owned_table(),
            rsb_mapping: obj.rsb_mapping().to_owned_table(),
        }
    }
}

impl<'a> FromTableRef<read_fonts::tables::hvar::Hvar<'a>> for Hvar {}

impl<'a> FontRead<'a> for Hvar {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        <read_fonts::tables::hvar::Hvar as FontRead>::read(data).map(|x| x.to_owned_table())
    }
}
