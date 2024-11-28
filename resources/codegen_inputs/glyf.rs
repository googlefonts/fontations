#![parse_module(read_fonts::tables::glyf)]
/// The [glyf (Glyph Data)](https://docs.microsoft.com/en-us/typography/opentype/spec/glyf) table
#[tag = "glyf"]
table Glyf {}

///// The [Glyph Header](https://docs.microsoft.com/en-us/typography/opentype/spec/glyf#glyph-headers)
//record GlyphHeader {
    ///// If the number of contours is greater than or equal to zero,
    ///// this is a simple glyph. If negative, this is a composite glyph
    ///// — the value -1 should be used for composite glyphs.
    //number_of_contours: i16,
    ///// Minimum x for coordinate data.
    //x_min: i16,
    ///// Minimum y for coordinate data.
    //y_min: i16,
    ///// Maximum x for coordinate data.
    //x_max: i16,
    ///// Maximum y for coordinate data.
    //y_max: i16,
//}


/// The [Glyph Header](https://docs.microsoft.com/en-us/typography/opentype/spec/glyf#glyph-headers)
table SimpleGlyph {
    /// If the number of contours is greater than or equal to zero,
    /// this is a simple glyph. If negative, this is a composite glyph
    /// — the value -1 should be used for composite glyphs.
    number_of_contours: i16,
    /// Minimum x for coordinate data.
    x_min: i16,
    /// Minimum y for coordinate data.
    y_min: i16,
    /// Maximum x for coordinate data.
    x_max: i16,
    /// Maximum y for coordinate data.
    y_max: i16,
    /// Array of point indices for the last point of each contour,
    /// in increasing numeric order
    #[count($number_of_contours)]
    end_pts_of_contours: [u16],
    /// Total number of bytes for instructions. If instructionLength is
    /// zero, no instructions are present for this glyph, and this
    /// field is followed directly by the flags field.
    instruction_length: u16,
    /// Array of instruction byte code for the glyph.
    #[count($instruction_length)]
    instructions: [u8],
    #[count(..)]
    //#[hidden]
    /// the raw data for flags & x/y coordinates
    glyph_data: [u8],

    ///// Array of flag elements. See below for details regarding the
    ///// number of flag array elements.
    //#[count(variable)]
    //flags: [SimpleGlyphFlags],
    ///// Contour point x-coordinates. See below for details regarding
    ///// the number of coordinate array elements. Coordinate for the
    ///// first point is relative to (0,0); others are relative to
    ///// previous point.
    //#[count(variable)]
    //x_coordinates: [uint8 or int16],
    ///// Contour point y-coordinates. See below for details regarding
    ///// the number of coordinate array elements. Coordinate for the
    ///// first point is relative to (0,0); others are relative to
    ///// previous point.
    //#[count(variable)]
    //y_coordinates: [uint8 or int16],
}

/// Flags used in [SimpleGlyph]
flags u8 SimpleGlyphFlags {
    /// Bit 0: If set, the point is on the curve; otherwise, it is off
    /// the curve.
    ON_CURVE_POINT = 0x01,
    /// Bit 1: If set, the corresponding x-coordinate is 1 byte long,
    /// and the sign is determined by the
    /// X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR flag. If not set, its
    /// interpretation depends on the
    /// X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR flag: If that other flag
    /// is set, the x-coordinate is the same as the previous
    /// x-coordinate, and no element is added to the xCoordinates
    /// array. If both flags are not set, the corresponding element in
    /// the xCoordinates array is two bytes and interpreted as a signed
    /// integer. See the description of the
    /// X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR flag for additional
    /// information.
    X_SHORT_VECTOR = 0x02,
    /// Bit 2: If set, the corresponding y-coordinate is 1 byte long,
    /// and the sign is determined by the
    /// Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR flag. If not set, its
    /// interpretation depends on the
    /// Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR flag: If that other flag
    /// is set, the y-coordinate is the same as the previous
    /// y-coordinate, and no element is added to the yCoordinates
    /// array. If both flags are not set, the corresponding element in
    /// the yCoordinates array is two bytes and interpreted as a signed
    /// integer. See the description of the
    /// Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR flag for additional
    /// information.
    Y_SHORT_VECTOR = 0x04,
    /// Bit 3: If set, the next byte (read as unsigned) specifies the
    /// number of additional times this flag byte is to be repeated in
    /// the logical flags array — that is, the number of additional
    /// logical flag entries inserted after this entry. (In the
    /// expanded logical array, this bit is ignored.) In this way, the
    /// number of flags listed can be smaller than the number of points
    /// in the glyph description.
    REPEAT_FLAG = 0x08,
    /// Bit 4: This flag has two meanings, depending on how the
    /// X_SHORT_VECTOR flag is set. If X_SHORT_VECTOR is set, this bit
    /// describes the sign of the value, with 1 equalling positive and
    /// 0 negative. If X_SHORT_VECTOR is not set and this bit is set,
    /// then the current x-coordinate is the same as the previous
    /// x-coordinate. If X_SHORT_VECTOR is not set and this bit is also
    /// not set, the current x-coordinate is a signed 16-bit delta
    /// vector.
    X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR = 0x10,
    /// Bit 5: This flag has two meanings, depending on how the
    /// Y_SHORT_VECTOR flag is set. If Y_SHORT_VECTOR is set, this bit
    /// describes the sign of the value, with 1 equalling positive and
    /// 0 negative. If Y_SHORT_VECTOR is not set and this bit is set,
    /// then the current y-coordinate is the same as the previous
    /// y-coordinate. If Y_SHORT_VECTOR is not set and this bit is also
    /// not set, the current y-coordinate is a signed 16-bit delta
    /// vector.
    Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR = 0x20,
    /// Bit 6: If set, contours in the glyph description may overlap.
    /// Use of this flag is not required in OpenType — that is, it is
    /// valid to have contours overlap without having this flag set. It
    /// may affect behaviors in some platforms, however. (See the
    /// discussion of “Overlapping contours” in Apple’s
    /// specification for details regarding behavior in Apple
    /// platforms.) When used, it must be set on the first flag byte
    /// for the glyph. See additional details below.
    OVERLAP_SIMPLE = 0x40,

    /// Bit 7: Off-curve point belongs to a cubic-Bezier segment
    /// 
    /// * [Spec](https://github.com/harfbuzz/boring-expansion-spec/blob/main/glyf1-cubicOutlines.md)
    /// * [harfbuzz](https://github.com/harfbuzz/harfbuzz/blob/c1ca46e4ebb6457dfe00a5441d52a4a66134ac58/src/OT/glyf/SimpleGlyph.hh#L23)
    CUBIC = 0x80,
}

/// [CompositeGlyph](https://docs.microsoft.com/en-us/typography/opentype/spec/glyf#glyph-headers)
table CompositeGlyph {
    /// If the number of contours is greater than or equal to zero,
    /// this is a simple glyph. If negative, this is a composite glyph
    /// — the value -1 should be used for composite glyphs.
    number_of_contours: i16,
    /// Minimum x for coordinate data.
    x_min: i16,
    /// Minimum y for coordinate data.
    y_min: i16,
    /// Maximum x for coordinate data.
    x_max: i16,
    /// Maximum y for coordinate data.
    y_max: i16,
    //header: GlyphHeader,
    /// component flag
    //flags: CompositeGlyphFlags,
    /// glyph index of component
    //glyph_index: u16,
    #[count(..)]
    component_data: [u8],

    ///// x-offset for component or point number; type depends on bits 0
    ///// and 1 in component flags
    //argument1: uint8, int8, uint16 or int16,
    ///// y-offset for component or point number; type depends on bits 0
    ///// and 1 in component flags
    //argument2: uint8, int8, uint16 or int16,
}

/// Flags used in [CompositeGlyph]
flags u16 CompositeGlyphFlags {
    /// Bit 0: If this is set, the arguments are 16-bit (uint16 or
    /// int16); otherwise, they are bytes (uint8 or int8).
    ARG_1_AND_2_ARE_WORDS = 0x0001,
    /// Bit 1: If this is set, the arguments are signed xy values,
    /// otherwise, they are unsigned point numbers.
    ARGS_ARE_XY_VALUES = 0x0002,
    /// Bit 2: If set and ARGS_ARE_XY_VALUES is also set, the xy values
    /// are rounded to the nearest grid line. Ignored if
    /// ARGS_ARE_XY_VALUES is not set.
    ROUND_XY_TO_GRID = 0x0004,
    /// Bit 3: This indicates that there is a simple scale for the
    /// component. Otherwise, scale = 1.0.
    WE_HAVE_A_SCALE = 0x0008,
    /// Bit 5: Indicates at least one more glyph after this one.
    MORE_COMPONENTS = 0x0020,
    /// Bit 6: The x direction will use a different scale from the y
    /// direction.
    WE_HAVE_AN_X_AND_Y_SCALE = 0x0040,
    /// Bit 7: There is a 2 by 2 transformation that will be used to
    /// scale the component.
    WE_HAVE_A_TWO_BY_TWO = 0x0080,
    /// Bit 8: Following the last component are instructions for the
    /// composite character.
    WE_HAVE_INSTRUCTIONS = 0x0100,
    /// Bit 9: If set, this forces the aw and lsb (and rsb) for the
    /// composite to be equal to those from this component glyph. This
    /// works for hinted and unhinted glyphs.
    USE_MY_METRICS = 0x0200,
    /// Bit 10: If set, the components of the compound glyph overlap.
    /// Use of this flag is not required in OpenType — that is, it is
    /// valid to have components overlap without having this flag set.
    /// It may affect behaviors in some platforms, however. (See
    /// Apple’s specification for details regarding behavior in Apple
    /// platforms.) When used, it must be set on the flag word for the
    /// first component. See additional remarks, above, for the similar
    /// OVERLAP_SIMPLE flag used in simple-glyph descriptions.
    OVERLAP_COMPOUND = 0x0400,
    /// Bit 11: The composite is designed to have the component offset
    /// scaled. Ignored if ARGS_ARE_XY_VALUES is not set.
    SCALED_COMPONENT_OFFSET = 0x0800,
    /// Bit 12: The composite is designed not to have the component
    /// offset scaled. Ignored if ARGS_ARE_XY_VALUES is not set.
    UNSCALED_COMPONENT_OFFSET = 0x1000,

    ///// Bits 4, 13, 14 and 15 are reserved: set to 0.
    //Reserved = 0xE010,
}

/// Simple or composite glyph.
format i16 Glyph {
    #[match_if($format >= 0)]
    Simple(SimpleGlyph),
    #[match_if($format < 0)]
    Composite(CompositeGlyph),
}

