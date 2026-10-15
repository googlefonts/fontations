//! impl subset() for hmtx and HMTX

use read_fonts::{
    tables::{
        hhea::{Hhea, HheaExtended},
        hmtx::{Hmtx, HmtxExtended},
    },
    TableProvider, TopLevelTable,
};

crate::metrics::subset_metrics_table!(Hmtx, Hhea, h_glyph_metric_records, hhea, false);
crate::metrics::subset_metrics_table!(
    HmtxExtended,
    HheaExtended,
    h_glyph_metric_records,
    hhea_extended,
    true
);
