
#![parse_module(read_fonts::tables::colr)]

/// [COLR (Color)](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#colr-header) table
table Colr {
    /// Table version number - set to 0 or 1.
    #[version]
    version: BigEndian<u16>,
    /// Number of BaseGlyph records; may be 0 in a version 1 table.
    num_base_glyph_records: BigEndian<u16>,
    /// Offset to baseGlyphRecords array (may be NULL).
    #[nullable]
    #[read_offset_with($num_base_glyph_records)]
    base_glyph_records_offset: BigEndian<Offset32<[BaseGlyph]>>,
    /// Offset to layerRecords array (may be NULL).
    #[nullable]
    #[read_offset_with($num_layer_records)]
    layer_records_offset: BigEndian<Offset32<[Layer]>>,
    /// Number of Layer records; may be 0 in a version 1 table.
    num_layer_records: BigEndian<u16>,
    /// Offset to BaseGlyphList table.
    #[available(1)]
    #[nullable]
    base_glyph_list_offset: BigEndian<Offset32<BaseGlyphList>>,
    /// Offset to LayerList table (may be NULL).
    #[available(1)]
    #[nullable]
    layer_list_offset: BigEndian<Offset32<LayerList>>,
    /// Offset to ClipList table (may be NULL).
    #[available(1)]
    #[nullable]
    clip_list_offset: BigEndian<Offset32<ClipList>>,
    /// Offset to DeltaSetIndexMap table (may be NULL).
    #[available(1)]
    #[nullable]
    var_index_map_offset: BigEndian<Offset32>,
    /// Offset to ItemVariationStore (may be NULL).
    #[available(1)]
    #[nullable]
    item_variation_store_offset: BigEndian<Offset32>,
}

/// [BaseGlyph](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#baseglyph-and-layer-records) record
record BaseGlyph {
    /// Glyph ID of the base glyph.
    glyph_id: BigEndian<GlyphId>,
    /// Index (base 0) into the layerRecords array.
    first_layer_index: BigEndian<u16>,
    /// Number of color layers associated with this glyph.
    num_layers: BigEndian<u16>,
}

/// [Layer](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#baseglyph-and-layer-records) record
record Layer {
    /// Glyph ID of the glyph used for a given layer.
    glyph_id: BigEndian<GlyphId>,
    /// Index (base 0) for a palette entry in the CPAL table.
    palette_index: BigEndian<u16>,
}

/// [BaseGlyphList](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#baseglyphlist-layerlist-and-cliplist) table
table BaseGlyphList {
    num_base_glyph_paint_records: BigEndian<u32>,
    #[count($num_base_glyph_paint_records)]
    base_glyph_paint_records: [BaseGlyphPaint],
}

/// [BaseGlyphPaint](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#baseglyphlist-layerlist-and-cliplist) record
record BaseGlyphPaint {
    /// Glyph ID of the base glyph.
    glyph_id: BigEndian<GlyphId>,
    /// Offset to a Paint table.
    paint_offset: BigEndian<Offset32<Paint>>,
}

/// [LayerList](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#baseglyphlist-layerlist-and-cliplist) table
table LayerList {
    num_layers: BigEndian<u32>,
    /// Offsets to Paint tables.
    #[count($num_layers)]
    paint_offsets: [BigEndian<Offset32<Paint>>],
}

/// [ClipList](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#baseglyphlist-layerlist-and-cliplist) table
table ClipList {
    /// Set to 1.
    format: BigEndian<u8>,
    /// Number of Clip records.
    num_clips: BigEndian<u32>,
    /// Clip records. Sorted by startGlyphID.
    #[count($num_clips)]
    clips: [Clip],
}

/// [Clip](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#baseglyphlist-layerlist-and-cliplist) record
record Clip {
    /// First glyph ID in the range.
    start_glyph_id: BigEndian<GlyphId>,
    /// Last glyph ID in the range.
    end_glyph_id: BigEndian<GlyphId>,
    /// Offset to a ClipBox table.
    clip_box_offset: BigEndian<Offset24<ClipBox>>,
}

/// [ClipBox](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#baseglyphlist-layerlist-and-cliplist) table
format u8 ClipBox {
    Format1(ClipBoxFormat1),
    Format2(ClipBoxFormat2),
}

/// [ClipBoxFormat1](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#baseglyphlist-layerlist-and-cliplist) record
table ClipBoxFormat1 {
    /// Set to 1.
    #[format = 1]
    format: BigEndian<u8>,
    /// Minimum x of clip box.
    x_min: BigEndian<FWord>,
    /// Minimum y of clip box.
    y_min: BigEndian<FWord>,
    /// Maximum x of clip box.
    x_max: BigEndian<FWord>,
    /// Maximum y of clip box.
    y_max: BigEndian<FWord>,
}

/// [ClipBoxFormat2](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#baseglyphlist-layerlist-and-cliplist) record
table ClipBoxFormat2 {
    /// Set to 2.
    #[format = 2]
    format: BigEndian<u8>,
    /// Minimum x of clip box. For variation, use varIndexBase + 0.
    x_min: BigEndian<FWord>,
    /// Minimum y of clip box. For variation, use varIndexBase + 1.
    y_min: BigEndian<FWord>,
    /// Maximum x of clip box. For variation, use varIndexBase + 2.
    x_max: BigEndian<FWord>,
    /// Maximum y of clip box. For variation, use varIndexBase + 3.
    y_max: BigEndian<FWord>,
    /// Base index into DeltaSetIndexMap.
    var_index_base: BigEndian<u32>,
}

/// [ColorIndex](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#color-references-colorstop-and-colorline) record
record ColorIndex {
    /// Index for a CPAL palette entry.
    palette_index: BigEndian<u16>,
    /// Alpha value.
    alpha: BigEndian<F2Dot14>,
}

/// [VarColorIndex](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#color-references-colorstop-and-colorline) record
record VarColorIndex {
    /// Index for a CPAL palette entry.
    palette_index: BigEndian<u16>,
    /// Alpha value. For variation, use varIndexBase + 0.
    alpha: BigEndian<F2Dot14>,
    /// Base index into DeltaSetIndexMap.
    var_index_base: BigEndian<u32>,
}

/// [ColorStop](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#color-references-colorstop-and-colorline) record
record ColorStop {
    /// Position on a color line.
    stop_offset: BigEndian<F2Dot14>,
    /// Index for a CPAL palette entry.
    palette_index: BigEndian<u16>,
    /// Alpha value.
    alpha: BigEndian<F2Dot14>,
}

/// [VarColorStop](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#color-references-colorstop-and-colorline) record
record VarColorStop {
    /// Position on a color line. For variation, use varIndexBase + 0.
    stop_offset: BigEndian<F2Dot14>,
    /// Index for a CPAL palette entry.
    palette_index: BigEndian<u16>,
    /// Alpha value. For variation, use varIndexBase + 1.
    alpha: BigEndian<F2Dot14>,
    /// Base index into DeltaSetIndexMap.
    var_index_base: BigEndian<u32>,
}

/// [ColorLine](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#color-references-colorstop-and-colorline) table
table ColorLine {
    /// An Extend enum value.
    extend: BigEndian<Extend>,
    /// Number of ColorStop records.
    num_stops: BigEndian<u16>,
    #[count($num_stops)]
    color_stops: [ColorStop],
}

/// [VarColorLine](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#color-references-colorstop-and-colorline) table
table VarColorLine {
    /// An Extend enum value.
    extend: BigEndian<Extend>,
    /// Number of ColorStop records.
    num_stops: BigEndian<u16>,
    /// Allows for variations.
    #[count($num_stops)]
    color_stops: [VarColorStop],
}

/// [Extend](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#color-references-colorstop-and-colorline) enumeration
enum u8 Extend {
    Pad = 0,
    Repeat = 1,
    Reflect = 2,
}

/// [Paint](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#paint-tables) tables
format u8 Paint {
    ColrLayers(PaintColrLayers),
    Solid(PaintSolid),
    VarSolid(PaintVarSolid),
    LinearGradient(PaintLinearGradient),
    VarLinearGradient(PaintVarLinearGradient),
    RadialGradient(PaintRadialGradient),
    VarRadialGradient(PaintVarRadialGradient),
    SweepGradient(PaintSweepGradient),
    VarSweepGradient(PaintVarSweepGradient),
    Glyph(PaintGlyph),
    ColrGlyph(PaintColrGlyph),
    Transform(PaintTransform),
    VarTransform(PaintVarTransform),
    Translate(PaintTranslate),
    VarTranslate(PaintVarTranslate),
    Scale(PaintScale),
    VarScale(PaintVarScale),
    ScaleAroundCenter(PaintScaleAroundCenter),
    VarScaleAroundCenter(PaintVarScaleAroundCenter),
    ScaleUniform(PaintScaleUniform),
    VarScaleUniform(PaintVarScaleUniform),
    ScaleUniformAroundCenter(PaintScaleUniformAroundCenter),
    VarScaleUniformAroundCenter(PaintVarScaleUniformAroundCenter),
    Rotate(PaintRotate),
    VarRotate(PaintVarRotate),
    RotateAroundCenter(PaintRotateAroundCenter),
    VarRotateAroundCenter(PaintVarRotateAroundCenter),
    Skew(PaintSkew),
    VarSkew(PaintVarSkew),
    SkewAroundCenter(PaintSkewAroundCenter),
    VarSkewAroundCenter(PaintVarSkewAroundCenter),
    Composite(PaintComposite),
}

/// [PaintColrLayers](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#format-1-paintcolrlayers) table
table PaintColrLayers {
    /// Set to 1.
    #[format = 1]
    format: BigEndian<u8>,
    /// Number of offsets to paint tables to read from LayerList.
    num_layers: BigEndian<u8>,
    /// Index (base 0) into the LayerList.
    first_layer_index: BigEndian<u32>,
}

/// [PaintSolid](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-2-and-3-paintsolid-paintvarsolid) table
table PaintSolid {
    /// Set to 2.
    #[format = 2]
    format: BigEndian<u8>,
    /// Index for a CPAL palette entry.
    palette_index: BigEndian<u16>,
    /// Alpha value.
    alpha: BigEndian<F2Dot14>,
}

/// [PaintVarSolid](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-2-and-3-paintsolid-paintvarsolid) table
table PaintVarSolid {
    /// Set to 3.
    #[format = 3]
    format: BigEndian<u8>,
    /// Index for a CPAL palette entry.
    palette_index: BigEndian<u16>,
    /// Alpha value. For variation, use varIndexBase + 0.
    alpha: BigEndian<F2Dot14>,
    /// Base index into DeltaSetIndexMap.
    var_index_base: BigEndian<u32>,
}

/// [PaintLinearGradient](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-4-and-5-paintlineargradient-paintvarlineargradient) table
table PaintLinearGradient {
    /// Set to 4.
    #[format = 4]
    format: BigEndian<u8>,
    /// Offset to ColorLine table.
    color_line_offset: BigEndian<Offset24<ColorLine>>,
    /// Start point (p₀) x coordinate.
    x0: BigEndian<FWord>,
    /// Start point (p₀) y coordinate.
    y0: BigEndian<FWord>,
    /// End point (p₁) x coordinate.
    x1: BigEndian<FWord>,
    /// End point (p₁) y coordinate.
    y1: BigEndian<FWord>,
    /// Rotation point (p₂) x coordinate.
    x2: BigEndian<FWord>,
    /// Rotation point (p₂) y coordinate.
    y2: BigEndian<FWord>,
}

/// [PaintVarLinearGradient](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-4-and-5-paintlineargradient-paintvarlineargradient) table
table PaintVarLinearGradient {
    /// Set to 5.
    #[format = 5]
    format: BigEndian<u8>,
    /// Offset to VarColorLine table.
    color_line_offset: BigEndian<Offset24<VarColorLine>>,
    /// Start point (p₀) x coordinate. For variation, use
    /// varIndexBase + 0.
    x0: BigEndian<FWord>,
    /// Start point (p₀) y coordinate. For variation, use
    /// varIndexBase + 1.
    y0: BigEndian<FWord>,
    /// End point (p₁) x coordinate. For variation, use varIndexBase
    /// + 2.
    x1: BigEndian<FWord>,
    /// End point (p₁) y coordinate. For variation, use varIndexBase
    /// + 3.
    y1: BigEndian<FWord>,
    /// Rotation point (p₂) x coordinate. For variation, use
    /// varIndexBase + 4.
    x2: BigEndian<FWord>,
    /// Rotation point (p₂) y coordinate. For variation, use
    /// varIndexBase + 5.
    y2: BigEndian<FWord>,
    /// Base index into DeltaSetIndexMap.
    var_index_base: BigEndian<u32>,
}

/// [PaintRadialGradient](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-6-and-7-paintradialgradient-paintvarradialgradient) table
table PaintRadialGradient {
    /// Set to 6.
    #[format = 6]
    format: BigEndian<u8>,
    /// Offset to ColorLine table.
    color_line_offset: BigEndian<Offset24<ColorLine>>,
    /// Start circle center x coordinate.
    x0: BigEndian<FWord>,
    /// Start circle center y coordinate.
    y0: BigEndian<FWord>,
    /// Start circle radius.
    radius0: BigEndian<UfWord>,
    /// End circle center x coordinate.
    x1: BigEndian<FWord>,
    /// End circle center y coordinate.
    y1: BigEndian<FWord>,
    /// End circle radius.
    radius1: BigEndian<UfWord>,
}

/// [PaintVarRadialGradient](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-6-and-7-paintradialgradient-paintvarradialgradient) table
table PaintVarRadialGradient {
    /// Set to 7.
    #[format = 7]
    format: BigEndian<u8>,
    /// Offset to VarColorLine table.
    color_line_offset: BigEndian<Offset24<VarColorLine>>,
    /// Start circle center x coordinate. For variation, use
    /// varIndexBase + 0.
    x0: BigEndian<FWord>,
    /// Start circle center y coordinate. For variation, use
    /// varIndexBase + 1.
    y0: BigEndian<FWord>,
    /// Start circle radius. For variation, use varIndexBase + 2.
    radius0: BigEndian<UfWord>,
    /// End circle center x coordinate. For variation, use varIndexBase
    /// + 3.
    x1: BigEndian<FWord>,
    /// End circle center y coordinate. For variation, use varIndexBase
    /// + 4.
    y1: BigEndian<FWord>,
    /// End circle radius. For variation, use varIndexBase + 5.
    radius1: BigEndian<UfWord>,
    /// Base index into DeltaSetIndexMap.
    var_index_base: BigEndian<u32>,
}

/// [PaintSweepGradient](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-8-and-9-paintsweepgradient-paintvarsweepgradient) table
table PaintSweepGradient {
    /// Set to 8.
    #[format = 8]
    format: BigEndian<u8>,
    /// Offset to ColorLine table.
    color_line_offset: BigEndian<Offset24<ColorLine>>,
    /// Center x coordinate.
    center_x: BigEndian<FWord>,
    /// Center y coordinate.
    center_y: BigEndian<FWord>,
    /// Start of the angular range of the gradient, 180° in
    /// counter-clockwise degrees per 1.0 of value.
    start_angle: BigEndian<F2Dot14>,
    /// End of the angular range of the gradient, 180° in
    /// counter-clockwise degrees per 1.0 of value.
    end_angle: BigEndian<F2Dot14>,
}

/// [PaintVarSweepGradient](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-8-and-9-paintsweepgradient-paintvarsweepgradient) table
table PaintVarSweepGradient {
    /// Set to 9.
    #[format = 9]
    format: BigEndian<u8>,
    /// Offset to VarColorLine table.
    color_line_offset: BigEndian<Offset24<VarColorLine>>,
    /// Center x coordinate. For variation, use varIndexBase + 0.
    center_x: BigEndian<FWord>,
    /// Center y coordinate. For variation, use varIndexBase + 1.
    center_y: BigEndian<FWord>,
    /// Start of the angular range of the gradient, 180° in
    /// counter-clockwise degrees per 1.0 of value. For variation, use
    /// varIndexBase + 2.
    start_angle: BigEndian<F2Dot14>,
    /// End of the angular range of the gradient, 180° in
    /// counter-clockwise degrees per 1.0 of value. For variation, use
    /// varIndexBase + 3.
    end_angle: BigEndian<F2Dot14>,
    /// Base index into DeltaSetIndexMap.
    var_index_base: BigEndian<u32>,
}

/// [PaintGlyph](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#format-10-paintglyph) table
table PaintGlyph {
    /// Set to 10.
    #[format = 10]
    format: BigEndian<u8>,
    /// Offset to a Paint table.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Glyph ID for the source outline.
    glyph_id: BigEndian<GlyphId>,
}

/// [PaintColrGlyph](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#format-11-paintcolrglyph) table
table PaintColrGlyph {
    /// Set to 11.
    #[format = 11]
    format: BigEndian<u8>,
    /// Glyph ID for a BaseGlyphList base glyph.
    glyph_id: BigEndian<GlyphId>,
}

/// [PaintTransform](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-12-and-13-painttransform-paintvartransform) table
table PaintTransform {
    /// Set to 12.
    #[format = 12]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Offset to an Affine2x3 table.
    transform_offset: BigEndian<Offset24<Affine2x3>>,
}

/// [PaintVarTransform](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-12-and-13-painttransform-paintvartransform) table
table PaintVarTransform {
    /// Set to 13.
    #[format = 13]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Offset to a VarAffine2x3 table.
    transform_offset: BigEndian<Offset24<VarAffine2x3>>,
}

/// [Affine2x3](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-12-and-13-painttransform-paintvartransform) record
table Affine2x3 {
    /// x-component of transformed x-basis vector.
    xx: BigEndian<Fixed>,
    /// y-component of transformed x-basis vector.
    yx: BigEndian<Fixed>,
    /// x-component of transformed y-basis vector.
    xy: BigEndian<Fixed>,
    /// y-component of transformed y-basis vector.
    yy: BigEndian<Fixed>,
    /// Translation in x direction.
    dx: BigEndian<Fixed>,
    /// Translation in y direction.
    dy: BigEndian<Fixed>,
}

/// [VarAffine2x3](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-12-and-13-painttransform-paintvartransform) record
table VarAffine2x3 {
    /// x-component of transformed x-basis vector. For variation, use
    /// varIndexBase + 0.
    xx: BigEndian<Fixed>,
    /// y-component of transformed x-basis vector. For variation, use
    /// varIndexBase + 1.
    yx: BigEndian<Fixed>,
    /// x-component of transformed y-basis vector. For variation, use
    /// varIndexBase + 2.
    xy: BigEndian<Fixed>,
    /// y-component of transformed y-basis vector. For variation, use
    /// varIndexBase + 3.
    yy: BigEndian<Fixed>,
    /// Translation in x direction. For variation, use varIndexBase + 4.
    dx: BigEndian<Fixed>,
    /// Translation in y direction. For variation, use varIndexBase + 5.
    dy: BigEndian<Fixed>,
    /// Base index into DeltaSetIndexMap.
    var_index_base: BigEndian<u32>,
}

/// [PaintTranslate](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-14-and-15-painttranslate-paintvartranslate) table
table PaintTranslate {
    /// Set to 14.
    #[format = 14]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Translation in x direction.
    dx: BigEndian<FWord>,
    /// Translation in y direction.
    dy: BigEndian<FWord>,
}

/// [PaintVarTranslate](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-14-and-15-painttranslate-paintvartranslate) table
table PaintVarTranslate {
    /// Set to 15.
    #[format = 15]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Translation in x direction. For variation, use varIndexBase + 0.
    dx: BigEndian<FWord>,
    /// Translation in y direction. For variation, use varIndexBase + 1.
    dy: BigEndian<FWord>,
    /// Base index into DeltaSetIndexMap.
    var_index_base: BigEndian<u32>,
}

/// [PaintScale](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-16-to-23-paintscale-and-variant-scaling-formats) table
table PaintScale {
    /// Set to 16.
    #[format = 16]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Scale factor in x direction.
    scale_x: BigEndian<F2Dot14>,
    /// Scale factor in y direction.
    scale_y: BigEndian<F2Dot14>,
}

/// [PaintVarScale](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-16-to-23-paintscale-and-variant-scaling-formats) table
table PaintVarScale {
    /// Set to 17.
    #[format = 17]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Scale factor in x direction. For variation, use varIndexBase +
    /// 0.
    scale_x: BigEndian<F2Dot14>,
    /// Scale factor in y direction. For variation, use varIndexBase +
    /// 1.
    scale_y: BigEndian<F2Dot14>,
    /// Base index into DeltaSetIndexMap.
    var_index_base: BigEndian<u32>,
}

/// [PaintScaleAroundCenter](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-16-to-23-paintscale-and-variant-scaling-formats) table
table PaintScaleAroundCenter {
    /// Set to 18.
    #[format = 18]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Scale factor in x direction.
    scale_x: BigEndian<F2Dot14>,
    /// Scale factor in y direction.
    scale_y: BigEndian<F2Dot14>,
    /// x coordinate for the center of scaling.
    center_x: BigEndian<FWord>,
    /// y coordinate for the center of scaling.
    center_y: BigEndian<FWord>,
}

/// [PaintVarScaleAroundCenter](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-16-to-23-paintscale-and-variant-scaling-formats) table
table PaintVarScaleAroundCenter {
    /// Set to 19.
    #[format = 19]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Scale factor in x direction. For variation, use varIndexBase +
    /// 0.
    scale_x: BigEndian<F2Dot14>,
    /// Scale factor in y direction. For variation, use varIndexBase +
    /// 1.
    scale_y: BigEndian<F2Dot14>,
    /// x coordinate for the center of scaling. For variation, use
    /// varIndexBase + 2.
    center_x: BigEndian<FWord>,
    /// y coordinate for the center of scaling. For variation, use
    /// varIndexBase + 3.
    center_y: BigEndian<FWord>,
    /// Base index into DeltaSetIndexMap.
    var_index_base: BigEndian<u32>,
}

/// [PaintScaleUniform](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-16-to-23-paintscale-and-variant-scaling-formats) table
table PaintScaleUniform {
    /// Set to 20.
    #[format = 20]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Scale factor in x and y directions.
    scale: BigEndian<F2Dot14>,
}

/// [PaintVarScaleUniform](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-16-to-23-paintscale-and-variant-scaling-formats) table
table PaintVarScaleUniform {
    /// Set to 21.
    #[format = 21]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Scale factor in x and y directions. For variation, use
    /// varIndexBase + 0.
    scale: BigEndian<F2Dot14>,
    /// Base index into DeltaSetIndexMap.
    var_index_base: BigEndian<u32>,
}

/// [PaintScaleUniformAroundCenter](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-16-to-23-paintscale-and-variant-scaling-formats) table
table PaintScaleUniformAroundCenter {
    /// Set to 22.
    #[format = 22]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Scale factor in x and y directions.
    scale: BigEndian<F2Dot14>,
    /// x coordinate for the center of scaling.
    center_x: BigEndian<FWord>,
    /// y coordinate for the center of scaling.
    center_y: BigEndian<FWord>,
}

/// [PaintVarScaleUniformAroundCenter](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-16-to-23-paintscale-and-variant-scaling-formats) table
table PaintVarScaleUniformAroundCenter {
    /// Set to 23.
    #[format = 23]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Scale factor in x and y directions. For variation, use
    /// varIndexBase + 0.
    scale: BigEndian<F2Dot14>,
    /// x coordinate for the center of scaling. For variation, use
    /// varIndexBase + 1.
    center_x: BigEndian<FWord>,
    /// y coordinate for the center of scaling. For variation, use
    /// varIndexBase + 2.
    center_y: BigEndian<FWord>,
    /// Base index into DeltaSetIndexMap.
    var_index_base: BigEndian<u32>,
}

/// [PaintRotate](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-24-to-27-paintrotate-paintvarrotate-paintrotatearoundcenter-paintvarrotatearoundcenter) table
table PaintRotate {
    /// Set to 24.
    #[format = 24]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Rotation angle, 180° in counter-clockwise degrees per 1.0 of
    /// value.
    angle: BigEndian<F2Dot14>,
}

/// [PaintVarRotate](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-24-to-27-paintrotate-paintvarrotate-paintrotatearoundcenter-paintvarrotatearoundcenter) table
table PaintVarRotate {
    /// Set to 25.
    #[format = 25]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Rotation angle, 180° in counter-clockwise degrees per 1.0 of
    /// value. For variation, use varIndexBase + 0.
    angle: BigEndian<F2Dot14>,
    /// Base index into DeltaSetIndexMap.
    var_index_base: BigEndian<u32>,
}

/// [PaintRotateAroundCenter](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-24-to-27-paintrotate-paintvarrotate-paintrotatearoundcenter-paintvarrotatearoundcenter) table
table PaintRotateAroundCenter {
    /// Set to 26.
    #[format = 26]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Rotation angle, 180° in counter-clockwise degrees per 1.0 of
    /// value.
    angle: BigEndian<F2Dot14>,
    /// x coordinate for the center of rotation.
    center_x: BigEndian<FWord>,
    /// y coordinate for the center of rotation.
    center_y: BigEndian<FWord>,
}

/// [PaintVarRotateAroundCenter](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-24-to-27-paintrotate-paintvarrotate-paintrotatearoundcenter-paintvarrotatearoundcenter) table
table PaintVarRotateAroundCenter {
    /// Set to 27.
    #[format = 27]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Rotation angle, 180° in counter-clockwise degrees per 1.0 of
    /// value. For variation, use varIndexBase + 0.
    angle: BigEndian<F2Dot14>,
    /// x coordinate for the center of rotation. For variation, use
    /// varIndexBase + 1.
    center_x: BigEndian<FWord>,
    /// y coordinate for the center of rotation. For variation, use
    /// varIndexBase + 2.
    center_y: BigEndian<FWord>,
    /// Base index into DeltaSetIndexMap.
    var_index_base: BigEndian<u32>,
}

/// [PaintSkew](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-28-to-31-paintskew-paintvarskew-paintskewaroundcenter-paintvarskewaroundcenter) table
table PaintSkew {
    /// Set to 28.
    #[format = 28]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Angle of skew in the direction of the x-axis, 180° in
    /// counter-clockwise degrees per 1.0 of value.
    x_skew_angle: BigEndian<F2Dot14>,
    /// Angle of skew in the direction of the y-axis, 180° in
    /// counter-clockwise degrees per 1.0 of value.
    y_skew_angle: BigEndian<F2Dot14>,
}

/// [PaintVarSkew](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-28-to-31-paintskew-paintvarskew-paintskewaroundcenter-paintvarskewaroundcenter) table
table PaintVarSkew {
    /// Set to 29.
    #[format = 29]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Angle of skew in the direction of the x-axis, 180° ┬░ in
    /// counter-clockwise degrees per 1.0 of value. For variation, use
    /// varIndexBase + 0.
    x_skew_angle: BigEndian<F2Dot14>,
    /// Angle of skew in the direction of the y-axis, 180° in
    /// counter-clockwise degrees per 1.0 of value. For variation, use
    /// varIndexBase + 1.
    y_skew_angle: BigEndian<F2Dot14>,
    /// Base index into DeltaSetIndexMap.
    var_index_base: BigEndian<u32>,
}

/// [PaintSkewAroundCenter](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-28-to-31-paintskew-paintvarskew-paintskewaroundcenter-paintvarskewaroundcenter) table
table PaintSkewAroundCenter {
    /// Set to 30.
    #[format = 30]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Angle of skew in the direction of the x-axis, 180° in
    /// counter-clockwise degrees per 1.0 of value.
    x_skew_angle: BigEndian<F2Dot14>,
    /// Angle of skew in the direction of the y-axis, 180° in
    /// counter-clockwise degrees per 1.0 of value.
    y_skew_angle: BigEndian<F2Dot14>,
    /// x coordinate for the center of rotation.
    center_x: BigEndian<FWord>,
    /// y coordinate for the center of rotation.
    center_y: BigEndian<FWord>,
}

/// [PaintVarSkewAroundCenter](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#formats-28-to-31-paintskew-paintvarskew-paintskewaroundcenter-paintvarskewaroundcenter) table
table PaintVarSkewAroundCenter {
    /// Set to 31.
    #[format = 31]
    format: BigEndian<u8>,
    /// Offset to a Paint subtable.
    paint_offset: BigEndian<Offset24<Paint>>,
    /// Angle of skew in the direction of the x-axis, 180° in
    /// counter-clockwise degrees per 1.0 of value. For variation, use
    /// varIndexBase + 0.
    x_skew_angle: BigEndian<F2Dot14>,
    /// Angle of skew in the direction of the y-axis, 180° in
    /// counter-clockwise degrees per 1.0 of value. For variation, use
    /// varIndexBase + 1.
    y_skew_angle: BigEndian<F2Dot14>,
    /// x coordinate for the center of rotation. For variation, use
    /// varIndexBase + 2.
    center_x: BigEndian<FWord>,
    /// y coordinate for the center of rotation. For variation, use
    /// varIndexBase + 3.
    center_y: BigEndian<FWord>,
    /// Base index into DeltaSetIndexMap.
    var_index_base: BigEndian<u32>,
}

/// [PaintComposite](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#format-32-paintcomposite) table
table PaintComposite {
    /// Set to 32.
    #[format = 32]
    format: BigEndian<u8>,
    /// Offset to a source Paint table.
    source_paint_offset: BigEndian<Offset24<Paint>>,
    /// A CompositeMode enumeration value.
    composite_mode: BigEndian<CompositeMode>,
    /// Offset to a backdrop Paint table.
    backdrop_paint_offset: BigEndian<Offset24<Paint>>,
}

/// [CompositeMode](https://learn.microsoft.com/en-us/typography/opentype/spec/colr#format-32-paintcomposite) enumeration
enum u8 CompositeMode {
    Clear = 0,
    Src = 1,
    Dest = 2,
    SrcOver = 3,
    DestOver = 4,
    SrcIn = 5,
    DestIn = 6,
    SrcOut = 7,
    DestOut = 8,
    SrcAtop = 9,
    DestAtop = 10,
    Xor = 11,
    Plus = 12,
    Screen = 13,
    Overlay = 14,
    Darken = 15,
    Lighten = 16,
    ColorDodge = 17,
    ColorBurn = 18,
    HardLight = 19,
    SoftLight = 20,
    Difference = 21,
    Exclusion = 22,
    Multiply = 23,
    HslHue = 24,
    HslSaturation = 25,
    HslColor = 26,
    HslLuminosity = 27,
}
