//! CFF hinting.

use read_fonts::{tables::postscript::dict::Blues, types::Fixed};

// "Default values for OS/2 typoAscender/Descender.."
// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psblues.h#L98>
const ICF_TOP: Fixed = Fixed::from_i32(880);
const ICF_BOTTOM: Fixed = Fixed::from_i32(-120);

// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psblues.h#L141>
const MAX_BLUES: usize = 7;
const MAX_OTHER_BLUES: usize = 5;
const MAX_BLUE_ZONES: usize = MAX_BLUES + MAX_OTHER_BLUES;

/// Parameters used to generate the stem and counter zones for the hinting
/// algorithm.
#[derive(Clone)]
pub(crate) struct HintParams {
    pub blues: Blues,
    pub family_blues: Blues,
    pub other_blues: Blues,
    pub family_other_blues: Blues,
    pub blue_scale: Fixed,
    pub blue_shift: Fixed,
    pub blue_fuzz: Fixed,
    pub language_group: i32,
}

impl Default for HintParams {
    fn default() -> Self {
        Self {
            blues: Blues::default(),
            other_blues: Blues::default(),
            family_blues: Blues::default(),
            family_other_blues: Blues::default(),
            // See <https://learn.microsoft.com/en-us/typography/opentype/spec/cff2#table-16-private-dict-operators>
            blue_scale: Fixed::from_f64(0.039625),
            blue_shift: Fixed::from_i32(7),
            blue_fuzz: Fixed::ONE,
            language_group: 0,
        }
    }
}

/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psblues.h#L129>
#[derive(Copy, Clone, PartialEq, Default, Debug)]
struct BlueZone {
    is_bottom: bool,
    cs_bottom_edge: Fixed,
    cs_top_edge: Fixed,
    cs_flat_edge: Fixed,
    ds_flat_edge: Fixed,
}

/// Hinting state for a PostScript subfont.
///
/// Note that hinter states depend on the scale, subfont index and
/// variation coordinates of a glyph. They can be retained and reused
/// if those values remain the same.
#[derive(Copy, Clone)]
pub(crate) struct HintState {
    scale: Fixed,
    blue_scale: Fixed,
    blue_shift: Fixed,
    blue_fuzz: Fixed,
    language_group: i32,
    supress_overshoot: bool,
    do_em_box_hints: bool,
    boost: Fixed,
    darken_y: Fixed,
    zones: [BlueZone; MAX_BLUE_ZONES],
    zone_count: usize,
}

impl HintState {
    pub fn new(params: &HintParams, scale: Fixed) -> Self {
        let mut state = Self {
            scale,
            blue_scale: params.blue_scale,
            blue_shift: params.blue_shift,
            blue_fuzz: params.blue_fuzz,
            language_group: params.language_group,
            supress_overshoot: false,
            do_em_box_hints: false,
            boost: Fixed::ZERO,
            darken_y: Fixed::ZERO,
            zones: [BlueZone::default(); MAX_BLUE_ZONES],
            zone_count: 0,
        };
        state.build_zones(params);
        state
    }

    #[cfg(test)]
    fn zones(&self) -> &[BlueZone] {
        &self.zones[..self.zone_count]
    }

    /// Initialize zones from the set of blues values.
    ///
    /// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psblues.c#L66>
    fn build_zones(&mut self, params: &HintParams) {
        self.do_em_box_hints = false;
        // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psblues.c#L141>
        match (self.language_group, params.blues.values().len()) {
            (1, 2) => {
                let blues = params.blues.values();
                if blues[0].0 < ICF_BOTTOM
                    && blues[0].1 < ICF_BOTTOM
                    && blues[1].0 > ICF_TOP
                    && blues[1].1 > ICF_TOP
                {
                    // FreeType generates synthetic hints here. We'll do it
                    // later when building the hint map.
                    self.do_em_box_hints = true;
                    return;
                }
            }
            (1, 0) => {
                self.do_em_box_hints = true;
                return;
            }
            _ => {}
        }
        let mut zones = [BlueZone::default(); MAX_BLUE_ZONES];
        let mut max_zone_height = Fixed::ZERO;
        let mut zone_ix = 0usize;
        // Copy blues and other blues to a combined array of top and bottom zones.
        for blue in params.blues.values().iter().take(MAX_BLUES) {
            // FreeType loads blues as integers and then expands to 16.16
            // at initialization. We load them as 16.16 so floor them here
            // to ensure we match.
            // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psblues.c#L190>
            let bottom = blue.0.floor();
            let top = blue.1.floor();
            let zone_height = top - bottom;
            if zone_height < Fixed::ZERO {
                // Reject zones with negative height
                continue;
            }
            max_zone_height = max_zone_height.max(zone_height);
            let zone = &mut zones[zone_ix];
            zone.cs_bottom_edge = bottom;
            zone.cs_top_edge = top;
            if zone_ix == 0 {
                // First blue value is bottom zone
                zone.is_bottom = true;
                zone.cs_flat_edge = top;
            } else {
                // Remaining blue values are top zones
                zone.is_bottom = false;
                // Adjust both edges of top zone upward by twice darkening amount
                zone.cs_top_edge += twice(self.darken_y);
                zone.cs_bottom_edge += twice(self.darken_y);
                zone.cs_flat_edge = zone.cs_bottom_edge;
            }
            zone_ix += 1;
        }
        for blue in params.other_blues.values().iter().take(MAX_OTHER_BLUES) {
            let bottom = blue.0.floor();
            let top = blue.1.floor();
            let zone_height = top - bottom;
            if zone_height < Fixed::ZERO {
                // Reject zones with negative height
                continue;
            }
            max_zone_height = max_zone_height.max(zone_height);
            let zone = &mut zones[zone_ix];
            // All "other" blues are bottom zone
            zone.is_bottom = true;
            zone.cs_bottom_edge = bottom;
            zone.cs_top_edge = top;
            zone.cs_flat_edge = top;
            zone_ix += 1;
        }
        // Adjust for family blues
        let units_per_pixel = Fixed::ONE / self.scale;
        for zone in &mut zones[..zone_ix] {
            let flat = zone.cs_flat_edge;
            let mut min_diff = Fixed::MAX;
            if zone.is_bottom {
                // In a bottom zone, the top edge is the flat edge.
                // Search family other blues for bottom zones. Look for the
                // closest edge that is within the one pixel threshold.
                for blue in params.family_other_blues.values() {
                    let family_flat = blue.1;
                    let diff = (flat - family_flat).abs();
                    if diff < min_diff && diff < units_per_pixel {
                        zone.cs_flat_edge = family_flat;
                        min_diff = diff;
                        if diff == Fixed::ZERO {
                            break;
                        }
                    }
                }
                // Check the first member of family blues, which is a bottom
                // zone
                if !params.family_blues.values().is_empty() {
                    let family_flat = params.family_blues.values()[0].1;
                    let diff = (flat - family_flat).abs();
                    if diff < min_diff && diff < units_per_pixel {
                        zone.cs_flat_edge = family_flat;
                    }
                }
            } else {
                // In a top zone, the bottom edge is the flat edge.
                // Search family blues for top zones, skipping the first, which
                // is a bottom zone. Look for closest family edge that is
                // within the one pixel threshold.
                for blue in params.family_blues.values().iter().skip(1) {
                    let family_flat = blue.0 + twice(self.darken_y);
                    let diff = (flat - family_flat).abs();
                    if diff < min_diff && diff < units_per_pixel {
                        zone.cs_flat_edge = family_flat;
                        min_diff = diff;
                        if diff == Fixed::ZERO {
                            break;
                        }
                    }
                }
            }
        }
        if max_zone_height > Fixed::ZERO && self.blue_scale > (Fixed::ONE / max_zone_height) {
            // Clamp at maximum scale
            self.blue_scale = Fixed::ONE / max_zone_height;
        }
        // Suppress overshoot and boost blue zones at small sizes
        if self.scale < self.blue_scale {
            self.supress_overshoot = true;
            self.boost =
                Fixed::from_f64(0.6) - Fixed::from_f64(0.6).mul_div(self.scale, self.blue_scale);
            // boost must remain less than 0.5, or baseline could go negative
            self.boost = self.boost.min(Fixed::from_bits(0x7FFF));
        }
        if self.darken_y != Fixed::ZERO {
            self.boost = Fixed::ZERO;
        }
        // Set device space alignment for each zone; apply boost amount before
        // rounding flat edge
        let scale = self.scale;
        let boost = self.boost;
        for zone in &mut zones[..zone_ix] {
            let boost = if zone.is_bottom { -boost } else { boost };
            zone.ds_flat_edge = (zone.cs_flat_edge * scale + boost).round();
        }
        self.zones = zones;
        self.zone_count = zone_ix;
    }
}

fn twice(value: Fixed) -> Fixed {
    Fixed::from_bits(value.to_bits().wrapping_mul(2))
}

#[cfg(test)]
mod tests {
    use super::{BlueZone, Blues, Fixed, HintParams, HintState};

    #[test]
    fn scaled_blue_zones() {
        fn make_blues(values: &[f64]) -> Blues {
            Blues::new(values.iter().copied().map(Fixed::from_f64))
        }
        // <BlueValues value="-15 0 536 547 571 582 714 726 760 772"/>
        // <OtherBlues value="-255 -240"/>
        // <BlueScale value="0.05"/>
        // <BlueShift value="7"/>
        // <BlueFuzz value="0"/>
        let params = HintParams {
            blues: make_blues(&[
                -15.0, 0.0, 536.0, 547.0, 571.0, 582.0, 714.0, 726.0, 760.0, 772.0,
            ]),
            other_blues: make_blues(&[-255.0, -240.0]),
            blue_scale: Fixed::from_f64(0.05),
            blue_shift: Fixed::from_i32(7),
            blue_fuzz: Fixed::ZERO,
            ..Default::default()
        };
        let state = HintState::new(&params, Fixed::ONE / Fixed::from_i32(64));
        assert!(!state.do_em_box_hints);
        assert_eq!(state.zone_count, 6);
        assert_eq!(state.boost, Fixed::from_bits(27035));
        assert!(state.supress_overshoot);
        // FreeType generates the following zones:
        let expected_zones = &[
            // csBottomEdge	-983040	int
            // csTopEdge	0	int
            // csFlatEdge	0	int
            // dsFlatEdge	0	int
            // bottomZone	1 '\x1'	unsigned char
            BlueZone {
                cs_bottom_edge: Fixed::from_bits(-983040),
                is_bottom: true,
                ..Default::default()
            },
            // csBottomEdge	35127296	int
            // csTopEdge	35848192	int
            // csFlatEdge	35127296	int
            // dsFlatEdge	589824	int
            // bottomZone	0 '\0'	unsigned char
            BlueZone {
                cs_bottom_edge: Fixed::from_bits(35127296),
                cs_top_edge: Fixed::from_bits(35848192),
                cs_flat_edge: Fixed::from_bits(35127296),
                ds_flat_edge: Fixed::from_bits(589824),
                is_bottom: false,
            },
            // csBottomEdge	37421056	int
            // csTopEdge	38141952	int
            // csFlatEdge	37421056	int
            // dsFlatEdge	589824	int
            // bottomZone	0 '\0'	unsigned char
            BlueZone {
                cs_bottom_edge: Fixed::from_bits(37421056),
                cs_top_edge: Fixed::from_bits(38141952),
                cs_flat_edge: Fixed::from_bits(37421056),
                ds_flat_edge: Fixed::from_bits(589824),
                is_bottom: false,
            },
            // csBottomEdge	46792704	int
            // csTopEdge	47579136	int
            // csFlatEdge	46792704	int
            // dsFlatEdge	786432	int
            // bottomZone	0 '\0'	unsigned char
            BlueZone {
                cs_bottom_edge: Fixed::from_bits(46792704),
                cs_top_edge: Fixed::from_bits(47579136),
                cs_flat_edge: Fixed::from_bits(46792704),
                ds_flat_edge: Fixed::from_bits(786432),
                is_bottom: false,
            },
            // csBottomEdge	49807360	int
            // csTopEdge	50593792	int
            // csFlatEdge	49807360	int
            // dsFlatEdge	786432	int
            // bottomZone	0 '\0'	unsigned char
            BlueZone {
                cs_bottom_edge: Fixed::from_bits(49807360),
                cs_top_edge: Fixed::from_bits(50593792),
                cs_flat_edge: Fixed::from_bits(49807360),
                ds_flat_edge: Fixed::from_bits(786432),
                is_bottom: false,
            },
            // csBottomEdge	-16711680	int
            // csTopEdge	-15728640	int
            // csFlatEdge	-15728640	int
            // dsFlatEdge	-262144	int
            // bottomZone	1 '\x1'	unsigned char
            BlueZone {
                cs_bottom_edge: Fixed::from_bits(-16711680),
                cs_top_edge: Fixed::from_bits(-15728640),
                cs_flat_edge: Fixed::from_bits(-15728640),
                ds_flat_edge: Fixed::from_bits(-262144),
                is_bottom: true,
            },
        ];
        assert_eq!(state.zones(), expected_zones);
    }
}
