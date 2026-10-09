//! impl subset() for hmtx and HMTX

use write_fonts::read::{
    tables::{
        hhea::{Hhea, HheaExtended},
        hmtx::{Hmtx, HmtxExtended},
    },
    TableProvider, TopLevelTable,
};

crate::metrics::subset_metrics_table!(Hmtx, Hhea, glyph_metric_records, hhea, false);
crate::metrics::subset_metrics_table!(
    HmtxExtended,
    HheaExtended,
    glyph_metric_records,
    hhea_extended,
    true
);
