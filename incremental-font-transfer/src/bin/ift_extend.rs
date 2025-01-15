//! IFT Extension
//!
//! This command line tool executes the IFT extension algorithm (<https://w3c.github.io/IFT/Overview.html#extend-font-subset>) on an IFT font.

use std::{
    collections::{BTreeSet, HashMap},
    str::FromStr,
};

use clap::Parser;
use font_types::{Fixed, Tag};
use incremental_font_transfer::{
    patch_group::{PatchGroup, UriStatus},
    patchmap::{DesignSpace, FeatureSet, SubsetDefinition},
};
use read_fonts::collections::{IntSet, RangeSet};
use regex::Regex;
use skrifa::FontRef;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Run the IFT extension algorithm (https://w3c.github.io/IFT/Overview.html#extend-font-subset) on an IFT font."
)]
struct Args {
    /// The input IFT font file.
    #[arg(short, long)]
    font: std::path::PathBuf,

    /// The output IFT font file.
    #[arg(short, long)]
    output: std::path::PathBuf,

    /// Text to extend the font to cover.
    #[arg(short, long)]
    text: Option<String>,

    /// Comma separate list of unicode codepoint values (base 10) to extend the font to cover.
    ///
    /// * indicates to include all unicode code points.
    #[arg(short, long, value_delimiter = ',', num_args = 1..)]
    unicodes: Vec<String>,

    /// Comma separate list of open type layout feature tags to extend the font to cover.
    ///
    /// * indicates to include all features.
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    features: Vec<String>,

    /// Comma separate list of design space ranges of the form tag@point or tag@start:end to extend the font to cover.
    ///
    /// * indicates to include all design spaces.
    ///
    /// For example wght at point 300 and wdth from 50 to 100:
    /// --design_space="wght@300,wdth@50:100"
    #[arg(short, long, value_delimiter = ',', num_args = 1..)]
    design_space: Vec<String>,
    // TODO(garretrieger): add feature tags arguments.
}

fn main() {
    let args = Args::parse();

    let mut codepoints = IntSet::<u32>::empty();
    if let Some(text) = args.text {
        codepoints.extend_unsorted(text.chars().map(|c| c as u32));
    }

    parse_unicodes(args.unicodes, &mut codepoints).expect("unicodes parsing failed.");
    let features = parse_features(args.features).expect("features parsing failed.");
    let design_space = parse_design_space(args.design_space).expect("design space parsing failed.");

    let subset_definition = SubsetDefinition::new(codepoints, features, design_space);

    let mut font_bytes = std::fs::read(&args.font).unwrap_or_else(|e| {
        panic!(
            "Unable to read input font file ({}): {:?}",
            args.font.display(),
            e
        )
    });

    let mut patch_data: HashMap<String, UriStatus> = Default::default();
    let mut it_count = 0;
    loop {
        it_count += 1;
        println!(">> Iteration {}", it_count);
        let font = FontRef::new(&font_bytes).expect("Input font parsing failed");
        let next_patches = PatchGroup::select_next_patches(font, &subset_definition)
            .expect("Patch selection failed");
        if !next_patches.has_uris() {
            println!("  No outstanding patches, all done.");
            break;
        }

        println!("  Selected URIs:");
        for uri in next_patches.uris() {
            println!("    fetching {}", uri);
            let uri_path = args.font.parent().unwrap().join(uri);
            let patch_bytes = std::fs::read(uri_path.clone()).unwrap_or_else(|e| {
                panic!(
                    "Unable to read patch file ({}): {:?}",
                    uri_path.display(),
                    e
                )
            });

            patch_data.insert(uri.to_string(), UriStatus::Pending(patch_bytes));
        }

        println!("  Applying patches");
        font_bytes = next_patches
            .apply_next_patches(&mut patch_data)
            .expect("Patch application failed.");
    }

    println!(">> Extension finished");
    std::fs::write(&args.output, font_bytes).expect("Writing output font failed.");
    println!(">> Wrote patched font to {}", &args.output.display());
}

fn parse_unicodes(args: Vec<String>, codepoints: &mut IntSet<u32>) -> Result<(), ParsingError> {
    for unicode_string in args {
        if unicode_string.is_empty() {
            continue;
        }

        if unicode_string == "*" {
            let all = IntSet::<u32>::all();
            codepoints.union(&all);
            return Ok(());
        }
        let Ok(unicode) = unicode_string.parse() else {
            return Err(ParsingError::UnicodeCodepointParsingFailed(unicode_string));
        };
        codepoints.insert(unicode);
    }
    Ok(())
}

fn parse_features(args: Vec<String>) -> Result<FeatureSet, ParsingError> {
    let mut tags = BTreeSet::<Tag>::new();
    for tag_string in args {
        let tag = match tag_string.as_str() {
            "" => continue,
            "*" => return Ok(FeatureSet::All),
            tag => Tag::new_checked(tag.as_bytes())
                .map_err(|_| ParsingError::FeatureTagParsingFailed(tag_string))?,
        };

        tags.insert(tag);
    }

    Ok(FeatureSet::Set(tags))
}

fn parse_fixed(value: &str, flag_value: &str) -> Result<Fixed, ParsingError> {
    f64::from_str(value)
        .map_err(|_| ParsingError::DesignSpaceParsingFailed {
            flag_value: flag_value.to_string(),
            message: "Bad axis position value".to_string(),
        })
        .map(Fixed::from_f64)
}

fn parse_design_space(args: Vec<String>) -> Result<DesignSpace, ParsingError> {
    let re = Regex::new(r"^([a-zA-Z][a-zA-Z0-9 ]{3})@([0-9.]+)(:[0-9.]+)?$").unwrap();

    let mut result = HashMap::<Tag, RangeSet<Fixed>>::default();
    for arg in args {
        if arg.is_empty() {
            continue;
        }

        if arg == "*" {
            return Ok(DesignSpace::All);
        }

        let Some(captures) = re.captures(&arg) else {
            return Err(ParsingError::DesignSpaceParsingFailed {
                flag_value: arg,
                message: "Invalid syntax. Must be tag@value or tag@value:value.".to_string(),
            });
        };

        let tag = captures.get(1).unwrap();
        let Ok(tag) = Tag::new_checked(tag.as_str().as_bytes()) else {
            return Err(ParsingError::DesignSpaceParsingFailed {
                flag_value: arg.clone(),
                message: format!("Bad tag value: {}", tag.as_str()),
            });
        };

        let value_1 = parse_fixed(captures.get(2).unwrap().as_str(), &arg)?;

        let range = if let Some(value_2) = captures.get(3) {
            let value_2 = parse_fixed(&value_2.as_str()[1..], &arg)?;
            value_1..=value_2
        } else {
            value_1..=value_1
        };

        result.entry(tag).or_default().insert(range);
    }

    Ok(DesignSpace::Ranges(result))
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsingError {
    DesignSpaceParsingFailed { flag_value: String, message: String },
    UnicodeCodepointParsingFailed(String),
    FeatureTagParsingFailed(String),
}

impl std::fmt::Display for ParsingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ParsingError::DesignSpaceParsingFailed {
                flag_value,
                message,
            } => {
                write!(
                    f,
                    "Failed parsing design_space flag ({}): {}",
                    flag_value, message
                )
            }
            ParsingError::UnicodeCodepointParsingFailed(value) => {
                write!(f, "Invalid unicode code point value: {}", value,)
            }
            ParsingError::FeatureTagParsingFailed(value) => {
                write!(f, "Invalid feature tag value: {value}")
            }
        }
    }
}

impl std::error::Error for ParsingError {}
