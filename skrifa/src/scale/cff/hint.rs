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

// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/pshints.h#L47>
const MAX_HINTS: usize = 96;

// One bit per stem hint
// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/pshints.h#L80>
const HINT_MASK_SIZE: usize = (MAX_HINTS + 7) / 8;

// Constant for hint adjustment and em box hint placement.
// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psblues.h#L114>
const MIN_COUNTER: Fixed = Fixed::from_bits(0x8000);

// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psfixed.h#L55>
const EPSILON: Fixed = Fixed::from_bits(1);

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
#[derive(Copy, Clone, PartialEq)]
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

    /// Check whether a hint is captured by one of the blue zones.
    ///
    /// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psblues.c#L465>
    fn capture(&self, bottom_edge: &mut Hint, top_edge: &mut Hint) -> bool {
        let fuzz = self.blue_fuzz;
        let mut captured = false;
        let mut adjustment = Fixed::ZERO;
        for zone in self.zones() {
            if zone.is_bottom
                && bottom_edge.is_bottom()
                && (zone.cs_bottom_edge - fuzz) <= bottom_edge.cs_coord
                && bottom_edge.cs_coord <= (zone.cs_top_edge + fuzz)
            {
                // Bottom edge captured by bottom zone.
                adjustment = if self.supress_overshoot {
                    zone.ds_flat_edge
                } else if zone.cs_top_edge - bottom_edge.cs_coord >= self.blue_shift {
                    // Guarantee minimum of 1 pixel overshoot
                    bottom_edge
                        .ds_coord
                        .round()
                        .min(zone.ds_flat_edge - Fixed::ONE)
                } else {
                    bottom_edge.ds_coord.round()
                };
                adjustment -= bottom_edge.ds_coord;
                captured = true;
                break;
            }
            if !zone.is_bottom
                && top_edge.is_top()
                && (zone.cs_bottom_edge - fuzz) <= top_edge.cs_coord
                && top_edge.cs_coord <= (zone.cs_top_edge + fuzz)
            {
                // Top edge captured by top zone.
                adjustment = if self.supress_overshoot {
                    zone.ds_flat_edge
                } else if top_edge.cs_coord - zone.cs_bottom_edge >= self.blue_shift {
                    // Guarantee minimum of 1 pixel overshoot
                    top_edge
                        .ds_coord
                        .round()
                        .max(zone.ds_flat_edge + Fixed::ONE)
                } else {
                    top_edge.ds_coord.round()
                };
                adjustment -= top_edge.ds_coord;
                captured = true;
                break;
            }
        }
        if captured {
            // Move both edges and mark them as "locked"
            if bottom_edge.is_valid() {
                bottom_edge.ds_coord += adjustment;
                bottom_edge.lock();
            }
            if top_edge.is_valid() {
                top_edge.ds_coord += adjustment;
                top_edge.lock();
            }
        }
        captured
    }
}

/// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/pshints.h#L85>
#[derive(Copy, Clone, Default)]
struct StemHint {
    /// If true, device space position is valid
    is_used: bool,
    // Character space position
    min: Fixed,
    max: Fixed,
    // Device space position after first use
    ds_min: Fixed,
    ds_max: Fixed,
}

// Hint flags
const GHOST_BOTTOM: u8 = 0x1;
const GHOST_TOP: u8 = 0x2;
const PAIR_BOTTOM: u8 = 0x4;
const PAIR_TOP: u8 = 0x8;
const LOCKED: u8 = 0x10;
const SYNTHETIC: u8 = 0x20;

/// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psblues.h#L118>
#[derive(Copy, Clone, PartialEq, Default, Debug)]
struct Hint {
    flags: u8,
    /// Index in original stem hint array (if not synthetic)
    index: u8,
    cs_coord: Fixed,
    ds_coord: Fixed,
    scale: Fixed,
}

impl Hint {
    fn is_valid(&self) -> bool {
        self.flags != 0
    }

    fn is_bottom(&self) -> bool {
        self.flags & (GHOST_BOTTOM | PAIR_BOTTOM) != 0
    }

    fn is_top(&self) -> bool {
        self.flags & (GHOST_TOP | PAIR_TOP) != 0
    }

    fn is_pair(&self) -> bool {
        self.flags & (PAIR_BOTTOM | PAIR_TOP) != 0
    }

    fn is_pair_top(&self) -> bool {
        self.flags & PAIR_TOP != 0
    }

    fn is_locked(&self) -> bool {
        self.flags & LOCKED != 0
    }

    fn is_synthetic(&self) -> bool {
        self.flags & SYNTHETIC != 0
    }

    fn lock(&mut self) {
        self.flags |= LOCKED
    }

    /// Hint initialization from an incoming stem hint.
    ///
    /// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/pshints.c#L89>
    fn setup(
        &mut self,
        stem: &StemHint,
        index: u8,
        origin: Fixed,
        scale: Fixed,
        darken_y: Fixed,
        is_bottom: bool,
    ) {
        // "Ghost hints" are used to align a single edge rather than a
        // stem-- think the top and bottom edges of an uppercase
        // sans-serif I.
        // These are encoded internally with stem hints of width -21
        // and -20 for bottom and top hints, respectively.
        const GHOST_BOTTOM_WIDTH: Fixed = Fixed::from_i32(-21);
        const GHOST_TOP_WIDTH: Fixed = Fixed::from_i32(-20);
        let width = stem.max - stem.min;
        if width == GHOST_BOTTOM_WIDTH {
            if is_bottom {
                self.cs_coord = stem.max;
                self.flags = GHOST_BOTTOM;
            } else {
                self.flags = 0;
            }
        } else if width == GHOST_TOP_WIDTH {
            if !is_bottom {
                self.cs_coord = stem.min;
                self.flags = GHOST_TOP;
            } else {
                self.flags = 0;
            }
        } else if width < Fixed::ZERO {
            // If width < 0, this is an inverted pair. We follow FreeType and
            // swap the coordinates
            if is_bottom {
                self.cs_coord = stem.max;
                self.flags = PAIR_BOTTOM;
            } else {
                self.cs_coord = stem.min;
                self.flags = PAIR_TOP;
            }
        } else {
            // This is a normal pair
            if is_bottom {
                self.cs_coord = stem.min;
                self.flags = PAIR_BOTTOM;
            } else {
                self.cs_coord = stem.max;
                self.flags = PAIR_TOP;
            }
        }
        if self.is_top() {
            // For top hints, adjust character space position up by twice the
            // darkening amount
            self.cs_coord += twice(darken_y);
        }
        self.cs_coord += origin;
        self.scale = scale;
        self.index = index;
        // If original stem hint was used, copy the position
        if self.flags != 0 && stem.is_used {
            if self.is_top() {
                self.ds_coord = stem.ds_max;
            } else {
                self.ds_coord = stem.ds_min;
            }
            self.lock();
        } else {
            self.ds_coord = self.cs_coord * scale;
        }
    }
}

/// Collection of adjusted hint edges.
///
/// <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/pshints.h#L126>
#[derive(Copy, Clone)]
struct HintMap {
    edges: [Hint; MAX_HINTS],
    len: usize,
    is_valid: bool,
    scale: Fixed,
}

impl HintMap {
    fn new(scale: Fixed) -> Self {
        Self {
            edges: [Hint::default(); MAX_HINTS],
            len: 0,
            is_valid: false,
            scale,
        }
    }

    fn clear(&mut self) {
        self.len = 0;
        self.is_valid = false;
    }

    /// Transform character space coordinate to device space.
    ///
    /// Based on <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/pshints.c#L331>
    fn transform(&self, coord: Fixed) -> Fixed {
        if self.len == 0 {
            return coord * self.scale;
        }
        let limit = self.len - 1;
        let mut i = 0;
        while i < limit && coord >= self.edges[i + 1].cs_coord {
            i += 1;
        }
        while i > 0 && coord < self.edges[i].cs_coord {
            i -= 1;
        }
        let first_edge = &self.edges[0];
        if i == 0 && coord < first_edge.cs_coord {
            // Special case for points below first edge: use uniform scale
            ((coord - first_edge.cs_coord) * self.scale) + first_edge.ds_coord
        } else {
            // Use highest edge where cs_coord >= edge.cs_coord
            let edge = &self.edges[i];
            ((coord - edge.cs_coord) * edge.scale) + edge.ds_coord
        }
    }

    /// Insert hint edges into map, sorted by character space coordinate.
    ///
    /// Based on <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/pshints.c#L606>
    fn insert(&mut self, bottom: &Hint, top: &Hint, initial: Option<&HintMap>) {
        let (is_pair, mut first_edge) = if !bottom.is_valid() {
            // Bottom is invalid: insert only top edge
            (false, *top)
        } else if !top.is_valid() {
            // Top is invalid: insert only bottom edge
            (false, *bottom)
        } else {
            // We have a valid pair!
            (true, *bottom)
        };
        let mut second_edge = *top;
        if is_pair && top.cs_coord < bottom.cs_coord {
            // Paired edges must be in proper order. FT just ignores the hint.
            return;
        }
        let edge_count = if is_pair { 2 } else { 1 };
        if self.len + edge_count > MAX_HINTS {
            // Won't fit. Again, ignore.
            return;
        }
        // Find insertion index that keeps the edge list sorted
        let mut insert_ix = 0;
        while insert_ix < self.len {
            if self.edges[insert_ix].cs_coord >= first_edge.cs_coord {
                break;
            }
            insert_ix += 1;
        }
        // Discard hints that overlap in character space
        if insert_ix < self.len {
            let current = &self.edges[insert_ix];
            // Existing edge is the same
            if (current.cs_coord == first_edge.cs_coord)
                // Pair straddles the next edge
                || (is_pair && current.cs_coord <= second_edge.cs_coord)
                // Inserting between paired edges 
                || current.is_pair_top()
            {
                return;
            }
        }
        // Recompute device space locations using initial hint map
        if !first_edge.is_locked() {
            if let Some(initial) = initial {
                if is_pair {
                    // Preserve stem width: position center of stem with
                    // initial hint map and two edges with nominal scale
                    let mid = initial.transform(half(second_edge.cs_coord + first_edge.cs_coord));
                    let half_width = half(second_edge.cs_coord - first_edge.cs_coord) * self.scale;
                    first_edge.ds_coord = mid - half_width;
                    second_edge.ds_coord = mid + half_width;
                } else {
                    first_edge.ds_coord = initial.transform(first_edge.cs_coord);
                }
            }
        }
        // Now discard hints that overlap in device space:
        if insert_ix > 0 && first_edge.ds_coord < self.edges[insert_ix - 1].ds_coord {
            // Inserting after an existing edge
            return;
        }
        if insert_ix < self.len
            && ((is_pair && second_edge.ds_coord > self.edges[insert_ix].ds_coord)
                || first_edge.ds_coord > self.edges[insert_ix].ds_coord)
        {
            // Inserting before an existing edge
            return;
        }
        // If we're inserting in the middle, make room in the edge array
        if insert_ix != self.len {
            let mut src_index = self.len - 1;
            let mut dst_index = self.len + edge_count - 1;
            loop {
                self.edges[dst_index] = self.edges[src_index];
                if src_index == insert_ix {
                    break;
                }
                src_index -= 1;
                dst_index -= 1;
            }
        }
        self.edges[insert_ix] = first_edge;
        if is_pair {
            self.edges[insert_ix + 1] = second_edge;
        }
        self.len += edge_count;
    }

    /// Adjust hint pairs so that one of the two edges is on a pixel boundary.
    ///
    /// Based on <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/pshints.c#L396>
    fn adjust(&mut self) {
        let mut saved = [(0usize, Fixed::ZERO); MAX_HINTS];
        let mut saved_count = 0usize;
        let mut i = 0;
        // From FT with adjustments for variable names:
        // "First pass is bottom-up (font hint order) without look-ahead.
        // Locked edges are already adjusted.
        // Unlocked edges begin with ds_coord from `initial_map'.
        // Save edges that are not optimally adjusted in `saved' array,
        // and process them in second pass."
        let limit = self.len;
        while i < limit {
            let is_pair = self.edges[i].is_pair();
            let j = if is_pair { i + 1 } else { i };
            if !self.edges[i].is_locked() {
                // We can adjust hint edges that are not locked
                let frac_down = self.edges[i].ds_coord.fract();
                let frac_up = self.edges[j].ds_coord.fract();
                // There are four possibilities. We compute them all.
                // (moves down are negative)
                let down_move_down = Fixed::ZERO - frac_down;
                let up_move_down = Fixed::ZERO - frac_up;
                let down_move_up = if frac_down == Fixed::ZERO {
                    Fixed::ZERO
                } else {
                    Fixed::ONE - frac_down
                };
                let up_move_up = if frac_up == Fixed::ZERO {
                    Fixed::ZERO
                } else {
                    Fixed::ONE - frac_up
                };
                // Smallest move up
                let move_up = down_move_up.min(up_move_up);
                // Smallest move down
                let move_down = down_move_down.max(up_move_down);
                let mut save_edge = false;
                let adjustment;
                // Check for room to move up:
                // 1. We're at the top of the array, or
                // 2. The next edge is at or above the proposed move up
                if j >= self.len - 1
                    || self.edges[j + 1].ds_coord
                        >= (self.edges[j].ds_coord + move_up + MIN_COUNTER)
                {
                    // Also check for room to move down...
                    if i == 0
                        || self.edges[i - 1].ds_coord
                            <= (self.edges[i].ds_coord + move_down - MIN_COUNTER)
                    {
                        // .. and move the smallest distance
                        adjustment = if -move_down < move_up {
                            move_down
                        } else {
                            move_up
                        };
                    } else {
                        adjustment = move_up;
                    }
                } else if i == 0
                    || self.edges[i - 1].ds_coord
                        <= (self.edges[i].ds_coord + move_down - MIN_COUNTER)
                {
                    // We can move down
                    adjustment = move_down;
                    // True if the move is not optimum
                    save_edge = move_up < -move_down;
                } else {
                    // We can't move either way without overlapping
                    adjustment = Fixed::ZERO;
                    save_edge = true;
                }
                // Capture non-optimal adjustments and save them for a second
                // pass. This is only possible if the edge above is unlocked
                // and can be moved.
                if save_edge && j < self.len - 1 && !self.edges[j + 1].is_locked() {
                    // (index, desired adjustment)
                    saved[saved_count] = (j, move_up - adjustment);
                    saved_count += 1;
                }
                // Apply the adjustment
                self.edges[i].ds_coord += adjustment;
                if is_pair {
                    self.edges[j].ds_coord += adjustment;
                }
            }
            // Compute the new edge scale
            if i > 0 && self.edges[i].cs_coord != self.edges[i - 1].cs_coord {
                let a = self.edges[i];
                let b = self.edges[i - 1];
                self.edges[i - 1].scale = (a.ds_coord - b.ds_coord) / (a.cs_coord - b.cs_coord);
            }
            if is_pair {
                if self.edges[j].cs_coord != self.edges[j - 1].cs_coord {
                    let a = self.edges[j];
                    let b = self.edges[j - 1];
                    self.edges[j - 1].scale = (a.ds_coord - b.ds_coord) / (a.cs_coord - b.cs_coord);
                }
                i += 1;
            }
            i += 1;
        }
        // Second pass tries to move non-optimal edges up if the first
        // pass created room
        for (j, adjustment) in saved[..saved_count].iter().copied().rev() {
            if self.edges[j + 1].ds_coord >= (self.edges[j].ds_coord + adjustment + MIN_COUNTER) {
                self.edges[j].ds_coord += adjustment;
                if self.edges[j].is_pair() {
                    self.edges[j - 1].ds_coord += adjustment;
                }
            }
        }
    }

    /// Builds a hintmap from hints and mask.
    ///
    /// If `initial_map` is invalid, this recurses one level to initialize
    /// it. If `is_initial` is true, simply build the initial map.
    ///
    /// Based on <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/pshints.c#L814>
    fn build(
        &mut self,
        state: &HintState,
        mask: Option<HintMask>,
        mut initial_map: Option<&mut HintMap>,
        stems: &mut [StemHint],
        origin: Fixed,
        is_initial: bool,
    ) {
        let scale = state.scale;
        let darken_y = Fixed::ZERO;
        if !is_initial {
            if let Some(initial_map) = &mut initial_map {
                if !initial_map.is_valid {
                    // Note: recursive call here to build the initial map if it
                    // is provided and invalid
                    initial_map.build(state, Some(HintMask::all()), None, stems, origin, true);
                }
            }
        }
        let initial_map = initial_map.map(|x| x as &HintMap);
        self.clear();
        // If the mask is missing or invalid, assume all hints are active
        let mut mask = mask.unwrap_or_else(HintMask::all);
        if !mask.is_valid {
            mask = HintMask::all();
        }
        if state.do_em_box_hints {
            // FreeType generates these during blues initialization. Do
            // it here just to avoid carrying the extra state in the
            // already large HintState struct.
            // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psblues.c#L160>
            let mut bottom = Hint::default();
            bottom.cs_coord = ICF_BOTTOM - EPSILON;
            bottom.ds_coord = (bottom.cs_coord * scale).round() - MIN_COUNTER;
            bottom.scale = scale;
            bottom.flags = GHOST_BOTTOM | LOCKED | SYNTHETIC;
            let mut top = Hint::default();
            top.cs_coord = ICF_TOP + EPSILON + twice(state.darken_y);
            top.ds_coord = (top.cs_coord * scale).round() + MIN_COUNTER;
            top.scale = scale;
            top.flags = GHOST_TOP | LOCKED | SYNTHETIC;
            let invalid = Hint::default();
            self.insert(&bottom, &invalid, initial_map);
            self.insert(&invalid, &top, initial_map);
        }
        let mut tmp_mask = mask;
        // FreeType iterates over the hint mask with some fancy bit logic. We
        // do the simpler thing and loop over the stems.
        // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/pshints.c#L897>
        for (i, stem) in stems.iter().enumerate() {
            if !tmp_mask.get(i) {
                continue;
            }
            let hint_ix = i as u8;
            let mut bottom = Hint::default();
            let mut top = Hint::default();
            bottom.setup(stem, hint_ix, origin, scale, darken_y, true);
            top.setup(stem, hint_ix, origin, scale, darken_y, false);
            // Insert hints that are locked or captured by a blue zone
            if bottom.is_locked() || top.is_locked() || state.capture(&mut bottom, &mut top) {
                if is_initial {
                    self.insert(&bottom, &top, None);
                } else {
                    self.insert(&bottom, &top, initial_map);
                }
                // Avoid processing this hint in the second pass
                tmp_mask.clear(i);
            }
        }
        if is_initial {
            // Heuristic: insert a point at (0, 0) if it's not covered by a
            // mapping. Ensures a lock at baseline for glyphs missing a
            // baseline hint.
            if self.len == 0
                || self.edges[0].cs_coord > Fixed::ZERO
                || self.edges[self.len - 1].cs_coord < Fixed::ZERO
            {
                let edge = Hint {
                    flags: GHOST_BOTTOM | LOCKED | SYNTHETIC,
                    scale,
                    ..Default::default()
                };
                let invalid = Hint::default();
                self.insert(&edge, &invalid, None);
            }
        } else {
            // Insert hints that were skipped in the first pass
            for (i, stem) in stems.iter().enumerate() {
                if !tmp_mask.get(i) {
                    continue;
                }
                let hint_ix = i as u8;
                let mut bottom = Hint::default();
                let mut top = Hint::default();
                bottom.setup(stem, hint_ix, origin, scale, darken_y, true);
                top.setup(stem, hint_ix, origin, scale, darken_y, false);
                self.insert(&bottom, &top, initial_map);
            }
        }
        // Adjust edges that are not locked to blue zones
        self.adjust();
        if !is_initial {
            // Save position of edges that were used by the hint map.
            for edge in &self.edges[..self.len] {
                if edge.is_synthetic() {
                    continue;
                }
                let stem = &mut stems[edge.index as usize];
                if edge.is_top() {
                    stem.ds_max = edge.ds_coord;
                } else {
                    stem.ds_min = edge.ds_coord;
                }
                stem.is_used = true;
            }
        }
        self.is_valid = true;
    }
}

/// Bitmask that specifies which hints are currently active.
///
/// "Each bit of the mask, starting with the most-significant bit of
/// the first byte, represents the corresponding hint zone in the
/// order in which the hints were declared at the beginning of
/// the charstring."
///
/// See <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=24>
/// Also <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/pshints.h#L70>
#[derive(Copy, Clone, PartialEq, Default)]
struct HintMask {
    mask: [u8; HINT_MASK_SIZE],
    is_valid: bool,
}

impl HintMask {
    fn new(bytes: &[u8]) -> Option<Self> {
        let len = bytes.len();
        if len > HINT_MASK_SIZE {
            return None;
        }
        let mut mask = Self::default();
        mask.mask[..len].copy_from_slice(&bytes[..len]);
        mask.is_valid = true;
        Some(mask)
    }

    fn all() -> Self {
        Self {
            mask: [0xFF; HINT_MASK_SIZE],
            is_valid: true,
        }
    }

    fn clear(&mut self, bit: usize) {
        self.mask[bit >> 3] &= !msb_mask(bit);
    }

    fn get(&self, bit: usize) -> bool {
        self.mask[bit >> 3] & msb_mask(bit) != 0
    }
}

/// Returns a bit mask for the selected bit with the
/// most significant bit at index 0.
fn msb_mask(bit: usize) -> u8 {
    1 << (7 - (bit & 0x7))
}

fn half(value: Fixed) -> Fixed {
    Fixed::from_bits(value.to_bits() / 2)
}

fn twice(value: Fixed) -> Fixed {
    Fixed::from_bits(value.to_bits().wrapping_mul(2))
}

#[cfg(test)]
mod tests {
    use super::{HintMap, StemHint, GHOST_BOTTOM, GHOST_TOP, LOCKED};
    use read_fonts::{types::F2Dot14, FontRef};

    use super::{
        BlueZone, Blues, Fixed, Hint, HintMask, HintParams, HintState, HINT_MASK_SIZE, PAIR_BOTTOM,
        PAIR_TOP,
    };

    fn make_hint_state() -> HintState {
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
        HintState::new(&params, Fixed::ONE / Fixed::from_i32(64))
    }

    #[test]
    fn scaled_blue_zones() {
        let state = make_hint_state();
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

    #[test]
    fn blue_zone_capture() {
        let state = make_hint_state();
        let bottom_edge = Hint {
            flags: PAIR_BOTTOM,
            ds_coord: Fixed::from_f64(2.3),
            ..Default::default()
        };
        let top_edge = Hint {
            flags: PAIR_TOP,
            // This value chosen to fit within the first "top" blue zone
            cs_coord: Fixed::from_bits(35127297),
            ds_coord: Fixed::from_f64(2.3),
            ..Default::default()
        };
        // Capture both
        {
            let (mut bottom_edge, mut top_edge) = (bottom_edge, top_edge);
            assert!(state.capture(&mut bottom_edge, &mut top_edge));
            assert!(bottom_edge.is_locked());
            assert!(top_edge.is_locked());
        }
        // Capture none
        {
            // Used to guarantee the edges are below all blue zones and will
            // not be captured
            let min_cs_coord = Fixed::MIN;
            let mut bottom_edge = Hint {
                cs_coord: min_cs_coord,
                ..bottom_edge
            };
            let mut top_edge = Hint {
                cs_coord: min_cs_coord,
                ..top_edge
            };
            assert!(!state.capture(&mut bottom_edge, &mut top_edge));
            assert!(!bottom_edge.is_locked());
            assert!(!top_edge.is_locked());
        }
        // Capture bottom, ignore invalid top
        {
            let mut bottom_edge = bottom_edge;
            let mut top_edge = Hint {
                // Empty flags == invalid hint
                flags: 0,
                ..top_edge
            };
            assert!(state.capture(&mut bottom_edge, &mut top_edge));
            assert!(bottom_edge.is_locked());
            assert!(!top_edge.is_locked());
        }
        // Capture top, ignore invalid bottom
        {
            let mut bottom_edge = Hint {
                // Empty flags == invalid hint
                flags: 0,
                ..bottom_edge
            };
            let mut top_edge = top_edge;
            assert!(state.capture(&mut bottom_edge, &mut top_edge));
            assert!(!bottom_edge.is_locked());
            assert!(top_edge.is_locked());
        }
    }

    #[test]
    fn hint_mask_ops() {
        const MAX_BITS: usize = HINT_MASK_SIZE * 8;
        let all_bits = HintMask::all();
        for i in 0..MAX_BITS {
            assert!(all_bits.get(i));
        }
        let odd_bits = HintMask::new(&[0b01010101; HINT_MASK_SIZE]).unwrap();
        for i in 0..MAX_BITS {
            assert_eq!(i & 1 != 0, odd_bits.get(i));
        }
        let mut cleared_bits = odd_bits;
        for i in 0..MAX_BITS {
            if i & 1 != 0 {
                cleared_bits.clear(i);
            }
        }
        assert_eq!(cleared_bits.mask, HintMask::default().mask);
    }

    #[test]
    fn hint_mapping() {
        let font = FontRef::new(font_test_data::CANTARELL_VF_TRIMMED).unwrap();
        let cff_font = super::super::Scaler::new(&font).unwrap();
        let state = cff_font
            .subfont(0, 8.0, &[F2Dot14::from_f32(-1.0); 2])
            .unwrap()
            .hint_state;
        let mut initial_map = HintMap::new(state.scale);
        let mut map = HintMap::new(state.scale);
        // Stem hints from Cantarell-VF.otf glyph id 2
        let mut stems = [
            StemHint {
                min: Fixed::from_bits(1376256),
                max: Fixed::ZERO,
                ..Default::default()
            },
            StemHint {
                min: Fixed::from_bits(16318464),
                max: Fixed::from_bits(17563648),
                ..Default::default()
            },
            StemHint {
                min: Fixed::from_bits(45481984),
                max: Fixed::from_bits(44171264),
                ..Default::default()
            },
        ];
        map.build(
            &state,
            Some(HintMask::all()),
            Some(&mut initial_map),
            &mut stems,
            Fixed::ZERO,
            false,
        );
        // FT generates the following hint map:
        //
        // index  csCoord  dsCoord  scale  flags
        //   0       0.00     0.00    526  gbL
        //   1     249.00   250.14    524  pb
        //   1     268.00   238.22    592  pt
        //   2     694.00   750.41    524  gtL
        let expected_edges = [
            Hint {
                index: 0,
                cs_coord: Fixed::from_f64(0.0),
                ds_coord: Fixed::from_f64(0.0),
                scale: Fixed::from_bits(526),
                flags: GHOST_BOTTOM | LOCKED,
            },
            Hint {
                index: 1,
                cs_coord: Fixed::from_bits(16318464),
                ds_coord: Fixed::from_bits(131072),
                scale: Fixed::from_bits(524),
                flags: PAIR_BOTTOM,
            },
            Hint {
                index: 1,
                cs_coord: Fixed::from_bits(17563648),
                ds_coord: Fixed::from_bits(141028),
                scale: Fixed::from_bits(592),
                flags: PAIR_TOP,
            },
            Hint {
                index: 2,
                cs_coord: Fixed::from_bits(45481984),
                ds_coord: Fixed::from_bits(393216),
                scale: Fixed::from_bits(524),
                flags: GHOST_TOP | LOCKED,
            },
        ];
        assert_eq!(expected_edges, &map.edges[..map.len]);
        // And FT generates the following mappings
        let mappings = [
            // (coord in font units, expected hinted coord in device space) in 16.16
            (0, 0),             // 0 -> 0
            (44302336, 382564), // 676 -> 5.828125
            (45481984, 393216), // 694 -> 6
            (16318464, 131072), // 249 -> 2
            (17563648, 141028), // 268 -> 2.140625
            (49676288, 426752), // 758 -> 6.5
            (56754176, 483344), // 866 -> 7.375
            (57868288, 492252), // 883 -> 7.5
            (50069504, 429896), // 764 -> 6.546875
        ];
        for (coord, expected) in mappings {
            assert_eq!(
                map.transform(Fixed::from_bits(coord)),
                Fixed::from_bits(expected)
            );
        }
    }
}
