//! impl subset() for vmtx and VMTX

use read_fonts::{
    tables::{
        vhea::{Vhea, VheaExtended},
        vmtx::{Vmtx, VmtxExtended},
    },
    TableProvider, TopLevelTable,
};

crate::metrics::subset_metrics_table!(Vmtx, Vhea, v_glyph_metric_records, vhea, false);
crate::metrics::subset_metrics_table!(
    VmtxExtended,
    VheaExtended,
    v_glyph_metric_records,
    vhea_extended,
    true
);
