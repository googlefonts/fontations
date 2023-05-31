//! Identifiers for specific tables
//!
//! These are used to record the type of certain serialized tables & subtables
//! that may require special attention while compiling the object graph.

use std::fmt::Display;

use font_types::Tag;
use read::TopLevelTable;

use crate::tables::layout::LookupType;

/// A marker for identifying the original source of various compiled tables.
///
/// In the general case, once a table has been compiled we do not need to know
/// what the bytes represent; however in certain special cases we do need this
/// information, in order to try alternate compilation strategies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum TableType {
    /// An unknown table
    #[default]
    Unknown,
    /// An untyped table generated for testing
    MockTable,
    /// A table with a given name (the name is used for debugging)
    Named(&'static str),
    /// A top-level table
    TopLevel(Tag),
    GposLookup(u16),
    GsubLookup(u16),
}

impl TableType {
    pub(crate) const GSUB: TableType = TableType::TopLevel(crate::tables::gsub::Gsub::TAG);
    pub(crate) const GPOS: TableType = TableType::TopLevel(crate::tables::gpos::Gpos::TAG);

    #[cfg(feature = "dot2")]
    pub(crate) fn is_mock(&self) -> bool {
        *self == TableType::MockTable
    }

    pub(crate) fn is_promotable(self) -> bool {
        match self {
            TableType::GposLookup(type_) => type_ != LookupType::GPOS_EXT_TYPE,
            TableType::GsubLookup(type_) => type_ != LookupType::GSUB_EXT_TYPE,
            _ => false,
        }
    }

    pub(crate) fn to_lookup_type(self) -> Option<LookupType> {
        match self {
            TableType::GposLookup(type_) => Some(LookupType::Gpos(type_)),
            TableType::GsubLookup(type_) => Some(LookupType::Gsub(type_)),
            _ => None,
        }
    }
}

impl From<LookupType> for TableType {
    fn from(src: LookupType) -> TableType {
        match src {
            LookupType::Gpos(type_) => TableType::GposLookup(type_),
            LookupType::Gsub(type_) => TableType::GsubLookup(type_),
        }
    }
}

impl Display for TableType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TableType::Unknown => write!(f, "Unknown"),
            TableType::MockTable => write!(f, "MockTable"),
            TableType::Named(name) => name.fmt(f),
            TableType::TopLevel(tag) => tag.fmt(f),
            TableType::GposLookup(gpos) => match gpos {
                1 => "GPOS1Single",
                2 => "GPOS2Pair",
                3 => "GPOS3Cursive",
                4 => "GPOS4MarkToBase",
                5 => "GPOS5MarkToLig",
                6 => "GPOS6MarkToMark",
                7 => "GPOS7Context",
                8 => "GPOS8Chain",
                9 => "GPOS9Extension",
                _ => unreachable!("never instantiated"),
            }
            .fmt(f),
            TableType::GsubLookup(gsub) => match gsub {
                1 => "GSUB1Single",
                2 => "GSUB2Multiple",
                3 => "GSUB3ALternate",
                4 => "GSUB4Ligature",
                5 => "GSUB5Context",
                6 => "GSUB6Chain",
                7 => "GSUB7Extension",
                8 => "GSUB8ReverseChain",
                _ => unreachable!("never instantiated"),
            }
            .fmt(f),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::tables::{gpos, gsub};
    use crate::FontWrite;

    use super::*;

    #[test]
    fn tagged_table_type() {
        assert_eq!(gsub::Gsub::default().table_type(), TableType::GSUB);
        assert_eq!(gpos::Gpos::default().table_type(), TableType::GPOS);

        assert_eq!(
            crate::tables::name::Name::default().table_type(),
            TableType::TopLevel(Tag::new(b"name"))
        );
    }

    #[test]
    fn promotable() {
        assert_eq!(
            gsub::SubstitutionLookup::Single(Default::default()).table_type(),
            TableType::GsubLookup(1)
        );
        assert_eq!(
            gsub::SubstitutionLookup::Extension(Default::default()).table_type(),
            TableType::GsubLookup(7)
        );
    }
}
