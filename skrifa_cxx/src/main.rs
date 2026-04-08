use read_fonts::ps::{
    cff::{
        charset::Charset as CffCharset, CffFontRef, Encoding as CffEncoding, Subfont as CffSubfont,
    },
    charmap::Charmap as PsCharmap,
    encoding::PredefinedEncoding,
    string::Sid,
    type1::Type1Font,
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

    #[derive(Copy, Clone, PartialEq, Eq, Debug)]
    pub enum PathVerb {
        MoveTo = 0,
        LineTo = 1,
        QuadTo = 2,
        CurveTo = 3,
        Close = 4,
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
        type PsFont<'a>;
        unsafe fn new_ps_font<'a>(data: &'a [u8]) -> Box<PsFont<'a>>;
        fn is_ok(&self) -> bool;
        fn num_glyphs(&self) -> u32;
        fn is_cid(&self) -> bool;
        fn cid_to_gid(&self, cid: u16) -> u32;
        fn unicode_to_gid(&self, unicode: u32) -> u32;
        fn encoding(&self) -> PsEncodingKind;
        fn code_to_gid(&self, code: u8) -> u32;
        fn scaled_outline(&self, gid: u32, ppem: f32, outline: &mut Outline) -> bool;
        fn unscaled_outline(&self, gid: u32, outline: &mut Outline) -> bool;
    }

    unsafe extern "C++" {
        include!("skrifa_cxx/src/outlines.h");

        fn run();
    }
}

use skrifa::{outline::OutlinePen, GlyphId};
use skrifa_ffi::{Outline, PathVerb, Point, PsEncodingKind};

pub enum PsFont<'a> {
    Error,
    Type1(Type1Font),
    Cff(CffFont<'a>),
}

pub struct CffFont<'a> {
    font: CffFontRef<'a>,
    charset: Option<CffCharset<'a>>,
    encoding: Option<CffEncoding<'a>>,
    unicode_cmap: Option<PsCharmap>,
    subfonts: Vec<Option<CffSubfont>>,
}

pub fn new_ps_font(data: &[u8]) -> Box<PsFont<'_>> {
    let font = if let Ok(cff) = CffFontRef::new(data, 0, None) {
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
        PsFont::Cff(CffFont {
            font: cff,
            charset,
            encoding,
            unicode_cmap,
            subfonts,
        })
    } else if let Ok(type1) = Type1Font::new(data) {
        PsFont::Type1(type1)
    } else {
        PsFont::Error
    };
    Box::new(font)
}

impl PsFont<'_> {
    fn is_ok(&self) -> bool {
        !matches!(self, Self::Error)
    }

    fn num_glyphs(&self) -> u32 {
        match self {
            Self::Type1(type1) => type1.num_glyphs(),
            Self::Cff(cff) => cff.font.num_glyphs(),
            Self::Error => 0,
        }
    }

    fn unicode_to_gid(&self, unicode: u32) -> u32 {
        let gid = match self {
            Self::Type1(type1) => type1.unicode_charmap().map(unicode),
            Self::Cff(cff) => cff.unicode_cmap.as_ref().and_then(|cmap| cmap.map(unicode)),
            Self::Error => return 0,
        };
        gid.unwrap_or_default().to_u32()
    }

    fn encoding(&self) -> PsEncodingKind {
        let maybe_predefined = match self {
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
            Self::Type1(type1) => type1.encoding().and_then(|encoding| encoding.map(code)),
            Self::Cff(cff) => cff
                .encoding
                .as_ref()
                .and_then(|encoding| encoding.map(code)),
            Self::Error => return 0,
        };
        gid.unwrap_or_default().to_u32()
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

impl OutlinePen for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.verbs.push(PathVerb::MoveTo);
        self.points.push(Point::new(x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.verbs.push(PathVerb::LineTo);
        self.points.push(Point::new(x, y));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.verbs.push(PathVerb::QuadTo);
        self.points.push(Point::new(cx0, cy0));
        self.points.push(Point::new(x, y));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.verbs.push(PathVerb::CurveTo);
        self.points.push(Point::new(cx0, cy0));
        self.points.push(Point::new(cx1, cy1));
        self.points.push(Point::new(x, y));
    }

    fn close(&mut self) {
        self.verbs.push(PathVerb::Close)
    }
}

fn main() {
    skrifa_ffi::run();
}
