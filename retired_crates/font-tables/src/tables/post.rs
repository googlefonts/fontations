//! the [post (PostScript)](https://docs.microsoft.com/en-us/typography/opentype/spec/post#header) table

use font_types::Tag;

/// 'post'
pub const TAG: Tag = Tag::new(b"post");

include!("../../generated/generated_post.rs");

impl<'a> Post<'a> {
    /// The number of glyph names covered by this table
    pub fn num_names(&self) -> usize {
        match self {
            Post::Post1_0(_) => DEFAULT_GLYPH_NAMES.len(),
            Post::Post2_0(table) => table.glyph_name_index().len() as usize,
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
