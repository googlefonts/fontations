/// the [post (PostScript)](https://docs.microsoft.com/en-us/typography/opentype/spec/post#header) table
use font_types::{BigEndian, FWord, Fixed, Tag, Version16Dot16};

/// 'post'
pub const TAG: Tag = Tag::new(b"post");

font_types::tables! {
    /// [post (PostScript)](https://docs.microsoft.com/en-us/typography/opentype/spec/post#header) table
    Post1_0 {
        /// 0x00010000 for version 1.0 0x00020000 for version 2.0
        /// 0x00025000 for version 2.5 (deprecated) 0x00030000 for version
        /// 3.0
        version: BigEndian<Version16Dot16>,
        /// Italic angle in counter-clockwise degrees from the vertical.
        /// Zero for upright text, negative for text that leans to the
        /// right (forward).
        italic_angle: BigEndian<Fixed>,
        /// This is the suggested distance of the top of the underline from
        /// the baseline (negative values indicate below baseline). The
        /// PostScript definition of this FontInfo dictionary key (the y
        /// coordinate of the center of the stroke) is not used for
        /// historical reasons. The value of the PostScript key may be
        /// calculated by subtracting half the underlineThickness from the
        /// value of this field.
        underline_position: BigEndian<FWord>,
        /// Suggested values for the underline thickness. In general, the
        /// underline thickness should match the thickness of the
        /// underscore character (U+005F LOW LINE), and should also match
        /// the strikeout thickness, which is specified in the OS/2 table.
        underline_thickness: BigEndian<FWord>,
        /// Set to 0 if the font is proportionally spaced, non-zero if the
        /// font is not proportionally spaced (i.e. monospaced).
        is_fixed_pitch: BigEndian<u32>,
        /// Minimum memory usage when an OpenType font is downloaded.
        min_mem_type42: BigEndian<u32>,
        /// Maximum memory usage when an OpenType font is downloaded.
        max_mem_type42: BigEndian<u32>,
        /// Minimum memory usage when an OpenType font is downloaded as a
        /// Type 1 font.
        min_mem_type1: BigEndian<u32>,
        /// Maximum memory usage when an OpenType font is downloaded as a
        /// Type 1 font.
        max_mem_type1: BigEndian<u32>,
    }

    /// [post (PostScript)](https://docs.microsoft.com/en-us/typography/opentype/spec/post#header) table
    Post2_0<'a> {
        /// 0x00010000 for version 1.0 0x00020000 for version 2.0
        /// 0x00025000 for version 2.5 (deprecated) 0x00030000 for version
        /// 3.0
        version: BigEndian<Version16Dot16>,
        /// Italic angle in counter-clockwise degrees from the vertical.
        /// Zero for upright text, negative for text that leans to the
        /// right (forward).
        italic_angle: BigEndian<Fixed>,
        /// This is the suggested distance of the top of the underline from
        /// the baseline (negative values indicate below baseline). The
        /// PostScript definition of this FontInfo dictionary key (the y
        /// coordinate of the center of the stroke) is not used for
        /// historical reasons. The value of the PostScript key may be
        /// calculated by subtracting half the underlineThickness from the
        /// value of this field.
        underline_position: BigEndian<FWord>,
        /// Suggested values for the underline thickness. In general, the
        /// underline thickness should match the thickness of the
        /// underscore character (U+005F LOW LINE), and should also match
        /// the strikeout thickness, which is specified in the OS/2 table.
        underline_thickness: BigEndian<FWord>,
        /// Set to 0 if the font is proportionally spaced, non-zero if the
        /// font is not proportionally spaced (i.e. monospaced).
        is_fixed_pitch: BigEndian<u32>,
        /// Minimum memory usage when an OpenType font is downloaded.
        min_mem_type42: BigEndian<u32>,
        /// Maximum memory usage when an OpenType font is downloaded.
        max_mem_type42: BigEndian<u32>,
        /// Minimum memory usage when an OpenType font is downloaded as a
        /// Type 1 font.
        min_mem_type1: BigEndian<u32>,
        /// Maximum memory usage when an OpenType font is downloaded as a
        /// Type 1 font.
        max_mem_type1: BigEndian<u32>,
        /// Number of glyphs (this should be the same as numGlyphs in
        /// 'maxp' table).
        #[hidden]
        num_glyphs: BigEndian<u16>,
        /// Array of indices into the string data. See below for details.
        #[count(num_glyphs)]
        glyph_name_index: [BigEndian<u16>],
        /// Storage for the string data.
        #[count_all]
        string_data: [u8],
    }

    #[format(Version16Dot16)]
    #[generate_getters]
    enum Post<'a> {
        #[version(Version16Dot16::VERSION_1_0)]
        Post1_0(Post1_0),
        #[version(Version16Dot16::VERSION_2_0)]
        Post2_0(Post2_0<'a>),
        #[version(Version16Dot16::VERSION_2_5)]
        Post2_5(Post1_0),
        #[version(Version16Dot16::VERSION_3_0)]
        Post3_0(Post1_0),
    }
}

impl<'a> Post<'a> {
    /// The number of glyph names covered by this table
    pub fn num_names(&self) -> usize {
        match self {
            Post::Post1_0(_) => DEFAULT_GLYPH_NAMES.len(),
            Post::Post2_0(table) => table.num_glyphs.get() as usize,
            _ => 0,
        }
    }

    pub fn glyph_name(&self, glyph_id: u16) -> Option<&str> {
        match self {
            Post::Post1_0(_table) => DEFAULT_GLYPH_NAMES.get(glyph_id as usize).copied(),
            Post::Post2_0(table) => {
                let idx = table.glyph_name_index().get(glyph_id as usize)?.get() as usize;
                if let Some(name) = DEFAULT_GLYPH_NAMES.get(idx) {
                    return Some(name);
                }
                let idx = idx - DEFAULT_GLYPH_NAMES.len();
                let mut offset = 0;
                for _ in 0..idx {
                    offset += table.string_data().get(offset).copied().unwrap_or(0) as usize + 1;
                }
                let len = table.string_data().get(offset).copied().unwrap_or(0) as usize;
                let bytes = table.string_data().get(offset + 1..offset + 1 + len)?;
                std::str::from_utf8(bytes).ok()
            }
            _ => None,
        }
    }
}

#[rustfmt::skip]
const DEFAULT_GLYPH_NAMES: [&str; 258] = [
    ".notdef", ".null", "nonmarkingreturn", "space", "exclam", "quotedbl", "numbersign", "dollar",
    "percent", "ampersand", "quotesingle", "parenleft", "parenright", "asterisk", "plus", "comma",
    "hyphen", "period", "slash", "zero", "one", "two", "three", "four", "five", "six", "seven",
    "eight", "nine", "colon", "semicolon", "less", "equal", "greater", "question", "at", "A", "B",
    "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U",
    "V", "W", "X", "Y", "Z", "bracketleft", "backslash", "bracketright", "asciicircum",
    "underscore", "grave", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n",
    "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z", "braceleft", "bar", "braceright",
    "asciitilde", "Adieresis", "Aring", "Ccedilla", "Eacute", "Ntilde", "Odieresis", "Udieresis",
    "aacute", "agrave", "acircumflex", "adieresis", "atilde", "aring", "ccedilla", "eacute",
    "egrave", "ecircumflex", "edieresis", "iacute", "igrave", "icircumflex", "idieresis", "ntilde",
    "oacute", "ograve", "ocircumflex", "odieresis", "otilde", "uacute", "ugrave", "ucircumflex",
    "udieresis", "dagger", "degree", "cent", "sterling", "section", "bullet", "paragraph",
    "germandbls", "registered", "copyright", "trademark", "acute", "dieresis", "notequal", "AE",
    "Oslash", "infinity", "plusminus", "lessequal", "greaterequal", "yen", "mu", "partialdiff",
    "summation", "product", "pi", "integral", "ordfeminine", "ordmasculine", "Omega", "ae",
    "oslash", "questiondown", "exclamdown", "logicalnot", "radical", "florin", "approxequal",
    "Delta", "guillemotleft", "guillemotright", "ellipsis", "nonbreakingspace", "Agrave", "Atilde",
    "Otilde", "OE", "oe", "endash", "emdash", "quotedblleft", "quotedblright", "quoteleft",
    "quoteright", "divide", "lozenge", "ydieresis", "Ydieresis", "fraction", "currency",
    "guilsinglleft", "guilsinglright", "fi", "fl", "daggerdbl", "periodcentered", "quotesinglbase",
    "quotedblbase", "perthousand", "Acircumflex", "Ecircumflex", "Aacute", "Edieresis", "Egrave",
    "Iacute", "Icircumflex", "Idieresis", "Igrave", "Oacute", "Ocircumflex", "apple", "Ograve",
    "Uacute", "Ucircumflex", "Ugrave", "dotlessi", "circumflex", "tilde", "macron", "breve",
    "dotaccent", "ring", "cedilla", "hungarumlaut", "ogonek", "caron", "Lslash", "lslash",
    "Scaron", "scaron", "Zcaron", "zcaron", "brokenbar", "Eth", "eth", "Yacute", "yacute", "Thorn",
    "thorn", "minus", "multiply", "onesuperior", "twosuperior", "threesuperior", "onehalf",
    "onequarter", "threequarters", "franc", "Gbreve", "gbreve", "Idotaccent", "Scedilla",
    "scedilla", "Cacute", "cacute", "Ccaron", "ccaron", "dcroat",
];
