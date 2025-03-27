//! GPOS subtable builders

use crate::tables::{
    gpos::{ValueFormat, ValueRecord},
    layout::builders::{DeviceOrDeltas, Metric},
    variations::ivs_builder::VariationStoreBuilder,
};

use super::AnchorTable;

/// A builder for ['ValueRecord`]s, which may contain raw deltas or device tables.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ValueRecordBuilder {
    /// The x advance, plus a possible device table or set of deltas
    pub x_advance: Option<Metric>,
    /// The y advance, plus a possible device table or set of deltas
    pub y_advance: Option<Metric>,
    /// The x placement, plus a possible device table or set of deltas
    pub x_placement: Option<Metric>,
    /// The y placement, plus a possible device table or set of deltas
    pub y_placement: Option<Metric>,
}

/// A builder for [`AnchorTable`]s, which may contain raw deltas or device tables.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AnchorBuilder {
    /// The x coordinate, plus a possible device table or set of deltas
    pub x: Metric,
    /// The y coordinate, plus a possible device table or set of deltas
    pub y: Metric,
    /// The countourpoint, in a format 2 anchor.
    ///
    /// This is a rarely used format.
    pub contourpoint: Option<u16>,
}

impl ValueRecordBuilder {
    /// Create a new all-zeros `ValueRecordBuilder`
    pub fn new() -> Self {
        Default::default()
    }

    /// Duplicates the x-advance value to x-placement, required for RTL rules.
    ///
    /// This is only necessary when a record was originally created without
    /// knowledge of the writing direction, and then later needs to be modified.
    pub fn make_rtl_compatible(&mut self) {
        if self.x_placement.is_none() {
            self.x_placement.clone_from(&self.x_advance);
        }
    }

    // these methods just match the existing builder methods on `ValueRecord`
    /// Builder style method to set the default x_placement value
    pub fn with_x_placement(mut self, val: i16) -> Self {
        self.x_placement
            .get_or_insert_with(Default::default)
            .default = val;
        self
    }

    /// Builder style method to set the default y_placement value
    pub fn with_y_placement(mut self, val: i16) -> Self {
        self.y_placement
            .get_or_insert_with(Default::default)
            .default = val;
        self
    }

    /// Builder style method to set the default x_placement value
    pub fn with_x_advance(mut self, val: i16) -> Self {
        self.x_advance.get_or_insert_with(Default::default).default = val;
        self
    }

    /// Builder style method to set the default y_placement value
    pub fn with_y_advance(mut self, val: i16) -> Self {
        self.y_advance.get_or_insert_with(Default::default).default = val;
        self
    }

    /// Builder style method to set the device or deltas for x_placement
    ///
    /// The argument can be a `Device` table or a `Vec<(VariationRegion, i16)>`
    pub fn with_x_placement_device(mut self, val: impl Into<DeviceOrDeltas>) -> Self {
        self.x_placement
            .get_or_insert_with(Default::default)
            .device_or_deltas = val.into();
        self
    }

    /// Builder style method to set the device or deltas for y_placement
    ///
    /// The argument can be a `Device` table or a `Vec<(VariationRegion, i16)>`
    pub fn with_y_placement_device(mut self, val: impl Into<DeviceOrDeltas>) -> Self {
        self.y_placement
            .get_or_insert_with(Default::default)
            .device_or_deltas = val.into();
        self
    }

    /// Builder style method to set the device or deltas for x_advance
    ///
    /// The argument can be a `Device` table or a `Vec<(VariationRegion, i16)>`
    pub fn with_x_advance_device(mut self, val: impl Into<DeviceOrDeltas>) -> Self {
        self.x_advance
            .get_or_insert_with(Default::default)
            .device_or_deltas = val.into();
        self
    }

    /// Builder style method to set the device or deltas for y_advance
    ///
    /// The argument can be a `Device` table or a `Vec<(VariationRegion, i16)>`
    pub fn with_y_advance_device(mut self, val: impl Into<DeviceOrDeltas>) -> Self {
        self.y_advance
            .get_or_insert_with(Default::default)
            .device_or_deltas = val.into();
        self
    }

    pub fn clear_zeros(mut self) -> Self {
        self.x_advance = self.x_advance.filter(|m| !m.is_zero());
        self.y_advance = self.y_advance.filter(|m| !m.is_zero());
        self.x_placement = self.x_placement.filter(|m| !m.is_zero());
        self.y_placement = self.y_placement.filter(|m| !m.is_zero());
        self
    }

    pub fn format(&self) -> ValueFormat {
        const EMPTY: ValueFormat = ValueFormat::empty();
        use ValueFormat as VF;

        let get_flags = |field: &Option<Metric>, def_flag, dev_flag| {
            let field = field.as_ref();
            let def_flag = if field.is_some() { def_flag } else { EMPTY };
            let dev_flag = field
                .and_then(|fld| (!fld.device_or_deltas.is_none()).then_some(dev_flag))
                .unwrap_or(EMPTY);
            (def_flag, dev_flag)
        };

        let (x_adv, x_adv_dev) = get_flags(&self.x_advance, VF::X_ADVANCE, VF::X_ADVANCE_DEVICE);
        let (y_adv, y_adv_dev) = get_flags(&self.y_advance, VF::Y_ADVANCE, VF::Y_ADVANCE_DEVICE);
        let (x_place, x_place_dev) =
            get_flags(&self.x_placement, VF::X_PLACEMENT, VF::X_PLACEMENT_DEVICE);
        let (y_place, y_place_dev) =
            get_flags(&self.y_placement, VF::Y_PLACEMENT, VF::Y_PLACEMENT_DEVICE);
        x_adv | y_adv | x_place | y_place | x_adv_dev | y_adv_dev | x_place_dev | y_place_dev
    }

    /// `true` if we are not null, but our set values are all 0
    pub fn is_all_zeros(&self) -> bool {
        let device_mask = ValueFormat::X_PLACEMENT_DEVICE
            | ValueFormat::Y_PLACEMENT_DEVICE
            | ValueFormat::X_ADVANCE_DEVICE
            | ValueFormat::Y_ADVANCE_DEVICE;

        let format = self.format();
        if format.is_empty() || format.intersects(device_mask) {
            return false;
        }
        let all_values = [
            &self.x_placement,
            &self.y_placement,
            &self.x_advance,
            &self.y_advance,
        ];
        all_values
            .iter()
            .all(|v| v.as_ref().map(|v| v.is_zero()).unwrap_or(true))
    }

    /// Build the final [`ValueRecord`], compiling deltas if needed.
    pub fn build(self, var_store: &mut VariationStoreBuilder) -> ValueRecord {
        let mut result = ValueRecord::new();
        result.x_advance = self.x_advance.as_ref().map(|val| val.default);
        result.y_advance = self.y_advance.as_ref().map(|val| val.default);
        result.x_placement = self.x_placement.as_ref().map(|val| val.default);
        result.y_placement = self.y_placement.as_ref().map(|val| val.default);
        result.x_advance_device = self
            .x_advance
            .and_then(|val| val.device_or_deltas.build(var_store))
            .into();
        result.y_advance_device = self
            .y_advance
            .and_then(|val| val.device_or_deltas.build(var_store))
            .into();
        result.x_placement_device = self
            .x_placement
            .and_then(|val| val.device_or_deltas.build(var_store))
            .into();
        result.y_placement_device = self
            .y_placement
            .and_then(|val| val.device_or_deltas.build(var_store))
            .into();

        result
    }
}

impl AnchorBuilder {
    /// Create a new [`AnchorBuilder`].
    pub fn new(x: i16, y: i16) -> Self {
        AnchorBuilder {
            x: x.into(),
            y: y.into(),
            contourpoint: None,
        }
    }

    /// Builder style method to set the device or deltas for the x value
    ///
    /// The argument can be a `Device` table or a `Vec<(VariationRegion, i16)>`
    pub fn with_x_device(mut self, val: impl Into<DeviceOrDeltas>) -> Self {
        self.x.device_or_deltas = val.into();
        self
    }

    /// Builder style method to set the device or deltas for the y value
    ///
    /// The argument can be a `Device` table or a `Vec<(VariationRegion, i16)>`
    pub fn with_y_device(mut self, val: impl Into<DeviceOrDeltas>) -> Self {
        self.y.device_or_deltas = val.into();
        self
    }

    /// Builder-style method to set the contourpoint.
    ///
    /// This is for the little-used format2 AnchorTable; it will be ignored
    /// if any device or deltas have been set.
    pub fn with_contourpoint(mut self, idx: u16) -> Self {
        self.contourpoint = Some(idx);
        self
    }

    /// Build the final [`AnchorTable`], adding deltas to the varstore if needed.
    pub fn build(self, var_store: &mut VariationStoreBuilder) -> AnchorTable {
        let x = self.x.default;
        let y = self.y.default;
        let x_dev = self.x.device_or_deltas.build(var_store);
        let y_dev = self.y.device_or_deltas.build(var_store);
        if x_dev.is_some() || y_dev.is_some() {
            AnchorTable::format_3(x, y, x_dev, y_dev)
        } else if let Some(point) = self.contourpoint {
            AnchorTable::format_2(x, y, point)
        } else {
            AnchorTable::format_1(x, y)
        }
    }
}
