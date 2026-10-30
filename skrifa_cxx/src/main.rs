use read_fonts::{
    model::pen::OutlinePen,
    ps::{
        cff::{
            charset::Charset as CffCharset, CffFontRef, Encoding as CffEncoding,
            Metadata as CffMetadata, Subfont as CffSubfont,
        },
        charmap::Charmap as PsCharmap,
        encoding::PredefinedEncoding,
        string::Sid,
        type1::Type1Font,
    },
    types::GlyphId,
};
use skrifa::{
    charmap::Charmap,
    instance::{LocationRef, Size},
    metrics::Metrics,
    outline::OutlineGlyphFormat,
    string::StringId,
    FontRef, GlyphNameSource, GlyphNames, MetadataProvider, OutlineGlyphCollection,
};

#[cxx::bridge(namespace = "skrifa")]
mod skrifa_ffi {
    #[derive(Copy, Clone, PartialEq, Eq, Debug)]
    pub enum PsEncodingKind {
        None = 0,
        Standard = 1,
        Expert = 2,
        IsoLatin1 = 3,
        Custom = 4,
    }

    // Should match SkPathVerb
    #[derive(Copy, Clone, PartialEq, Eq, Debug)]
    #[repr(u8)]
    pub enum PathVerb {
        MoveTo = 0,
        LineTo = 1,
        QuadTo = 2,
        CurveTo = 4,
        Close = 5,
    }

    #[derive(Copy, Clone, PartialEq, Debug)]
    pub struct Point {
        pub x: f32,
        pub y: f32,
    }

    #[derive(Clone, Debug)]
    pub struct Outline {
        pub verbs: Vec<PathVerb>,
        pub points: Vec<Point>,
        pub advance_width: f32,
    }

    extern "Rust" {
        type SkrifaFont<'a>;
        unsafe fn new_font<'a>(data: &'a [u8], index: u32) -> Box<SkrifaFont<'a>>;
        fn is_ok(&self) -> bool;
        fn font_type(&self) -> &'static str;
        unsafe fn postscript_name<'a>(&'a self) -> &'a str;
        unsafe fn family_name<'a>(&'a self) -> &'a str;
        fn units_per_em(&self) -> i32;
        fn ascent(&self) -> f32;
        fn descent(&self) -> f32;
        fn num_glyphs(&self) -> u32;
        fn is_fixed_pitch(&self) -> bool;
        fn is_cid(&self) -> bool;
        fn cid_to_gid(&self, cid: u16) -> u32;
        fn unicode_to_gid(&self, unicode: u32) -> u32;
        fn encoding(&self) -> PsEncodingKind;
        fn code_to_gid(&self, code: u8) -> u32;
        fn has_glyph_names(&self) -> bool;
        fn glyph_name(&self, gid: u32) -> String;
        fn scaled_outline(&self, gid: u32, ppem: f32, outline: &mut Outline) -> bool;
        fn unscaled_outline(&self, gid: u32, outline: &mut Outline) -> bool;

        fn agl_name_to_unicode(name: &str, unicode: &mut u32) -> bool;
        fn agl_unicode_to_name(unicode: u32, name: &mut [u8]) -> bool;
    }

    unsafe extern "C++" {
        include!("skrifa_cxx/src/outlines.h");

        fn run();
    }
}

use skrifa_ffi::{Outline, PathVerb, Point, PsEncodingKind};

pub enum SkrifaFont<'a> {
    Sfnt(Sfnt<'a>),
    Type1(Type1Font),
    Cff(CffFont<'a>),
    Error,
}

pub struct Sfnt<'a> {
    font: FontRef<'a>,
    metrics: Metrics,
    ps_name: Option<String>,
    family_name: Option<String>,
    glyph_names: GlyphNames<'a>,
    charmap: Charmap<'a>,
    outlines: OutlineGlyphCollection<'a>,
}

impl<'a> Sfnt<'a> {
    fn new(data: &'a [u8], index: u32) -> Option<Self> {
        let font = FontRef::from_index(data, index).ok()?;
        let metrics = font.metrics(Size::unscaled(), LocationRef::default());
        let get_name = |id| {
            font.localized_strings(id)
                .english_or_first()
                .map(|s| s.to_string())
        };
        let ps_name = get_name(StringId::POSTSCRIPT_NAME);
        let family_name =
            get_name(StringId::FAMILY_NAME).or_else(|| get_name(StringId::TYPOGRAPHIC_FAMILY_NAME));
        let glyph_names = font.glyph_names();
        let charmap = font.charmap();
        let outlines = font.outline_glyphs();
        Some(Self {
            font,
            ps_name,
            metrics,
            family_name,
            glyph_names,
            charmap,
            outlines,
        })
    }
}

pub struct CffFont<'a> {
    font: CffFontRef<'a>,
    meta: Option<CffMetadata<'a>>,
    charset: Option<CffCharset<'a>>,
    encoding: Option<CffEncoding<'a>>,
    unicode_cmap: Option<PsCharmap>,
    subfonts: Vec<Option<CffSubfont>>,
}

pub fn new_font(data: &[u8], index: u32) -> Box<SkrifaFont<'_>> {
    if let Some(sfnt) = Sfnt::new(data, index) {
        return Box::new(SkrifaFont::Sfnt(sfnt));
    }
    let font = if let Ok(cff) = CffFontRef::new(data, index, None) {
        let meta = cff.metadata();
        let charset = cff.charset();
        let encoding = cff.encoding();
        let subfonts = (0..cff.num_subfonts())
            .map(|i| cff.subfont(i, &[]).ok())
            .collect();
        let unicode_cmap = if let Some(charset) = charset.as_ref() {
            Some(PsCharmap::from_glyph_names(charset.iter().filter_map(
                |(gid, sid)| Some((gid, core::str::from_utf8(cff.string(sid)?).ok()?)),
            )))
        } else {
            None
        };
        SkrifaFont::Cff(CffFont {
            font: cff,
            meta,
            charset,
            encoding,
            unicode_cmap,
            subfonts,
        })
    } else if let Ok(type1) = Type1Font::new(data) {
        SkrifaFont::Type1(type1)
    } else {
        SkrifaFont::Error
    };
    Box::new(font)
}

impl SkrifaFont<'_> {
    fn is_ok(&self) -> bool {
        !matches!(self, Self::Error)
    }

    fn font_type(&self) -> &'static str {
        match self {
            Self::Sfnt(sfnt) => match sfnt.outlines.format() {
                Some(OutlineGlyphFormat::Glyf) => "TrueType",
                Some(OutlineGlyphFormat::Cff) | Some(OutlineGlyphFormat::Cff2) => "CFF",
                _ => "",
            },
            Self::Type1(_) => "Type 1",
            Self::Cff(_) => "CFF",
            Self::Error => "",
        }
    }

    fn postscript_name(&self) -> &str {
        match self {
            Self::Sfnt(sfnt) => sfnt.ps_name.as_deref().unwrap_or_default(),
            Self::Type1(type1) => type1.name().unwrap_or_default(),
            Self::Cff(cff) => cff
                .meta
                .as_ref()
                .and_then(|meta| meta.name())
                .unwrap_or_default(),
            Self::Error => "",
        }
    }

    fn family_name(&self) -> &str {
        match self {
            Self::Sfnt(sfnt) => sfnt.family_name.as_deref().unwrap_or_default(),
            Self::Type1(type1) => type1.family_name().unwrap_or_default(),
            Self::Cff(cff) => cff
                .meta
                .as_ref()
                .and_then(|meta| meta.family_name())
                .unwrap_or_default(),
            Self::Error => "",
        }
    }

    fn units_per_em(&self) -> i32 {
        match self {
            Self::Sfnt(sfnt) => sfnt.metrics.units_per_em as i32,
            Self::Type1(type1) => type1.upem(),
            Self::Cff(cff) => cff.font.upem(),
            Self::Error => 0,
        }
    }

    fn ascent(&self) -> f32 {
        let bbox = match self {
            Self::Sfnt(sfnt) => return sfnt.metrics.ascent,
            Self::Type1(type1) => type1.bbox(),
            Self::Cff(cff) => cff
                .meta
                .as_ref()
                .map(|meta| meta.bbox())
                .unwrap_or_default(),
            Self::Error => return 0.0,
        };
        bbox.y_max.to_f32()
    }

    fn descent(&self) -> f32 {
        let bbox = match self {
            Self::Sfnt(sfnt) => return sfnt.metrics.descent,
            Self::Type1(type1) => type1.bbox(),
            Self::Cff(cff) => cff
                .meta
                .as_ref()
                .map(|meta| meta.bbox())
                .unwrap_or_default(),
            Self::Error => return 0.0,
        };
        bbox.y_min.to_f32()
    }

    fn num_glyphs(&self) -> u32 {
        match self {
            Self::Sfnt(sfnt) => sfnt.metrics.glyph_count as u32,
            Self::Type1(type1) => type1.num_glyphs(),
            Self::Cff(cff) => cff.font.num_glyphs(),
            Self::Error => 0,
        }
    }

    fn is_fixed_pitch(&self) -> bool {
        match self {
            Self::Sfnt(sfnt) => sfnt.metrics.is_monospace,
            Self::Type1(type1) => type1.is_fixed_pitch(),
            Self::Cff(cff) => cff
                .meta
                .as_ref()
                .map(|meta| meta.is_fixed_pitch())
                .unwrap_or(false),
            Self::Error => false,
        }
    }

    fn unicode_to_gid(&self, unicode: u32) -> u32 {
        let gid = match self {
            Self::Sfnt(sfnt) => sfnt.charmap.map(unicode),
            Self::Type1(type1) => type1.unicode_charmap().map(unicode),
            Self::Cff(cff) => cff.unicode_cmap.as_ref().and_then(|cmap| cmap.map(unicode)),
            Self::Error => return 0,
        };
        gid.unwrap_or_default().to_u32()
    }

    fn encoding(&self) -> PsEncodingKind {
        let maybe_predefined = match self {
            Self::Sfnt(_sfnt) => return PsEncodingKind::None,
            Self::Type1(type1) => type1.encoding().map(|encoding| encoding.predefined()),
            Self::Cff(cff) => cff.encoding.as_ref().map(|encoding| encoding.predefined()),
            Self::Error => return PsEncodingKind::None,
        };
        let Some(predefined) = maybe_predefined else {
            return PsEncodingKind::None;
        };
        match predefined {
            Some(PredefinedEncoding::Standard) => PsEncodingKind::Standard,
            Some(PredefinedEncoding::Expert) => PsEncodingKind::Custom,
            Some(PredefinedEncoding::IsoLatin1) => PsEncodingKind::IsoLatin1,
            None => PsEncodingKind::Custom,
        }
    }

    fn code_to_gid(&self, code: u8) -> u32 {
        let gid = match self {
            Self::Sfnt(_sfnt) => return 0,
            Self::Type1(type1) => type1.encoding().and_then(|encoding| encoding.map(code)),
            Self::Cff(cff) => cff
                .encoding
                .as_ref()
                .and_then(|encoding| encoding.map(code)),
            Self::Error => return 0,
        };
        let gid = gid.unwrap_or_default().to_u32();
        if gid < self.num_glyphs() {
            gid
        } else {
            0
        }
    }

    fn has_glyph_names(&self) -> bool {
        match self {
            Self::Sfnt(sfnt) => sfnt.glyph_names.source() != GlyphNameSource::Synthesized,
            Self::Type1(_type1) => true,
            Self::Cff(cff) => cff.charset.is_some() && !cff.font.is_cid(),
            Self::Error => false,
        }
    }

    fn glyph_name(&self, gid: u32) -> String {
        match self {
            Self::Sfnt(sfnt) => sfnt
                .glyph_names
                .get(GlyphId::new(gid))
                .map(|s| s.to_string())
                .unwrap_or_default(),
            Self::Type1(type1) => type1.glyph_name(gid.into()).unwrap_or_default().to_string(),
            Self::Cff(cff) if cff.charset.is_some() && !cff.font.is_cid() => cff
                .charset
                .as_ref()
                .and_then(|charset| charset.string_id(gid.into()).ok())
                .and_then(|sid| cff.font.string(sid))
                .and_then(|s| core::str::from_utf8(s).ok())
                .map(|s| s.to_string())
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    fn is_cid(&self) -> bool {
        match self {
            Self::Cff(cff) => cff.font.is_cid(),
            _ => false,
        }
    }

    fn cid_to_gid(&self, cid: u16) -> u32 {
        match self {
            Self::Cff(cff) if cff.font.is_cid() => cff
                .charset
                .as_ref()
                .and_then(|charset| charset.glyph_id(Sid::new(cid)).ok())
                .unwrap_or_default()
                .to_u32(),
            _ => 0,
        }
    }

    fn scaled_outline(&self, gid: u32, ppem: f32, outline: &mut Outline) -> bool {
        self.outline_impl(gid, Some(ppem), outline).is_some()
    }

    fn unscaled_outline(&self, gid: u32, outline: &mut Outline) -> bool {
        self.outline_impl(gid, None, outline).is_some()
    }

    fn outline_impl(&self, gid: u32, ppem: Option<f32>, outline: &mut Outline) -> Option<()> {
        outline.verbs.clear();
        outline.points.clear();
        outline.advance_width = 0.0;
        let width = match self {
            Self::Sfnt(sfnt) => {
                let gid = GlyphId::new(gid);
                let size = ppem.map(Size::new).unwrap_or(Size::unscaled());
                let glyph = sfnt.outlines.get(gid)?;
                let metrics = glyph.draw(size, outline).ok()?;
                metrics.advance_width.unwrap_or_else(|| {
                    sfnt.font
                        .glyph_metrics(size, LocationRef::default())
                        .advance_width(gid)
                        .unwrap_or_default()
                })
            }
            Self::Type1(type1) => type1.draw(gid.into(), ppem, outline).ok()??,
            Self::Cff(cff) => {
                let gid = GlyphId::new(gid);
                let subfont = cff
                    .subfonts
                    .get(cff.font.subfont_index(gid)? as usize)?
                    .as_ref()?;
                cff.font.draw(subfont, gid, &[], ppem, outline).ok()??
            }
            Self::Error => return None,
        };
        outline.advance_width = width;
        Some(())
    }
}

impl Point {
    fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl Outline {
    fn push<const N: usize>(&mut self, verb: PathVerb, points: [(f32, f32); N]) {
        self.verbs.push(verb);
        self.points
            .extend(points.into_iter().map(|(x, y)| Point::new(x, y)));
    }
}

impl OutlinePen for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.push(PathVerb::MoveTo, [(x, y)]);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.push(PathVerb::LineTo, [(x, y)]);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.push(PathVerb::QuadTo, [(cx0, cy0), (x, y)]);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.push(PathVerb::CurveTo, [(cx0, cy0), (cx1, cy1), (x, y)]);
    }

    fn close(&mut self) {
        self.push(PathVerb::Close, []);
    }
}

fn agl_name_to_unicode(name: &str, unicode: &mut u32) -> bool {
    if let Some(uni) = read_fonts::ps::agl::name_to_char(name) {
        *unicode = uni as u32;
        true
    } else {
        false
    }
}

fn agl_unicode_to_name(unicode: u32, name: &mut [u8]) -> bool {
    read_fonts::ps::agl::char_to_name(unicode, name).is_some()
}

fn main() {
    skrifa_ffi::run();
}
