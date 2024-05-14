//! binary subset tool
//!
//! Takes a font file and a subset input which describes the desired subset, and ouput is a new
//! font file containing only the data specified in the input.
//!

use clap::Parser;
use int_set::IntSet;
use klippa::{Plan, Subset};
use thiserror::Error;
use write_fonts::read::{FontRef, TableProvider, TopLevelTable};
use write_fonts::types::GlyphId;
use write_fonts::{
    from_obj::FromTableRef, tables::hhea::Hhea, tables::hmtx::Hmtx, tables::maxp::Maxp, FontBuilder,
};

#[derive(Error, Debug)]
pub enum InvalidInputError {
    #[error("Invalid input gid {0}")]
    InvalidGid(String),

    #[error("Invalid gid range {start}-{end}")]
    InvalidGidRange { start: u16, end: u16 },

    #[error("Invalid input unicode {0}")]
    InvalidUnicode(String),

    #[error("Invalid unicode range {start}-{end}")]
    InvalidUnicodeRange { start: u32, end: u32 },
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The input font file.
    #[arg(short, long)]
    path: std::path::PathBuf,

    /// List of glyph ids
    #[arg(short, long)]
    gids: Option<String>,

    /// List of unicode codepoints
    #[arg(short, long)]
    unicodes: Option<String>,

    /// The output font file
    #[arg(short, long)]
    output_file: std::path::PathBuf,
}

fn main() {
    let args = Args::parse();

    let gids = match populate_gids(&args.gids) {
        Ok(gids) => gids,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let unicodes = match parse_unicodes(&args.unicodes) {
        Ok(unicodes) => unicodes,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let font_bytes = std::fs::read(&args.path).expect("Invalid input font file found");
    let font = FontRef::new(&font_bytes).expect("Error reading font bytes");
    let plan = Plan::new(&gids, &unicodes, &font);

    let hmtx = font.hmtx().expect("Error reading hmtx table");
    let mut hmtx = Hmtx::from_table_ref(&hmtx);
    hmtx.subset(&plan).expect("SUbsetting failed");
    let hmtx_bytes = write_fonts::dump_table(&hmtx).unwrap();

    let hhea = font.hhea().expect("Error reading hhea table");
    let mut hhea = Hhea::from_table_ref(&hhea);
    hhea.subset(&plan).expect("Subsetting failed");
    let hhea_bytes = write_fonts::dump_table(&hhea).unwrap();

    let maxp = font.maxp().expect("Error reading maxp table");
    let mut maxp = Maxp::from_table_ref(&maxp);
    maxp.subset(&plan).expect("Subsetting failed");
    let maxp_bytes = write_fonts::dump_table(&maxp).unwrap();

    let mut builder = FontBuilder::default();
    builder.add_raw(Hmtx::TAG, hmtx_bytes);
    builder.add_raw(Hhea::TAG, hhea_bytes);
    builder.add_raw(Maxp::TAG, maxp_bytes);

    builder.copy_missing_tables(font);

    std::fs::write(&args.output_file, builder.build()).unwrap();
}

fn populate_gids(gid_str: &Option<String>) -> Result<IntSet<GlyphId>, InvalidInputError> {
    let mut result = IntSet::empty();
    let Some(gid_str) = gid_str else {
        return Ok(result);
    };
    for gid in gid_str.split(',') {
        if let Some((start, end)) = gid.split_once('-') {
            let start: u16 = start
                .parse::<u16>()
                .map_err(|_| InvalidInputError::InvalidGid(start.to_owned().clone()))?;
            let end: u16 = end
                .parse::<u16>()
                .map_err(|_| InvalidInputError::InvalidGid(end.to_owned().clone()))?;
            if start > end {
                return Err(InvalidInputError::InvalidGidRange { start, end });
            }
            result.extend((start..=end).map(GlyphId::new));
        } else {
            let glyph_id: u16 = gid
                .parse::<u16>()
                .map_err(|_| InvalidInputError::InvalidGid(gid.to_owned().clone()))?;
            result.insert(GlyphId::new(glyph_id));
        }
    }
    Ok(result)
}

fn parse_unicodes(unicode_str: &Option<String>) -> Result<IntSet<u32>, InvalidInputError> {
    let mut result = IntSet::empty();
    let Some(unicode_str) = unicode_str else {
        return Ok(result);
    };
    let re = regex::Regex::new(r"[<+->{},;&#\\xXuUnNiI\n\t\v\f\r]").unwrap();
    let s = re.replace_all(unicode_str, " ");
    for cp in s.split_whitespace() {
        if let Some((start, end)) = cp.split_once('-') {
            let start: u32 = start
                .parse::<u32>()
                .map_err(|_| InvalidInputError::InvalidUnicode(start.to_owned()))?;
            let end: u32 = end
                .parse::<u32>()
                .map_err(|_| InvalidInputError::InvalidUnicode(end.to_owned()))?;
            if start > end {
                return Err(InvalidInputError::InvalidUnicodeRange { start, end });
            }
            result.extend(start..=end);
        } else {
            let unicode: u32 = cp
                .parse::<u32>()
                .map_err(|_| InvalidInputError::InvalidUnicode(cp.to_owned()))?;
            result.insert(unicode);
        }
    }
    Ok(result)
}
