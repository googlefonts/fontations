//! impl subset() for vmtx and VMTX

use write_fonts::read::{
    tables::{
        vhea::{Vhea, VheaExtended},
        vmtx::{Vmtx, VmtxExtended},
    },
    TableProvider, TopLevelTable,
};

crate::metrics::subset_metrics_table!(Vmtx, Vhea, vertical_glyph_metric_records, vhea, false);
crate::metrics::subset_metrics_table!(
    VmtxExtended,
    VheaExtended,
    vertical_glyph_metric_records,
    vhea_extended,
    true
);
