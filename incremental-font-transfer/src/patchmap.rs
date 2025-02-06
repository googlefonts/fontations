//! Loads incremental font transfer <https://w3c.github.io/IFT/Overview.html> patch mappings.
//!
//! The IFT and IFTX tables encode mappings from subset definitions to URL's which host patches
//! that can be applied to the font to add support for the corresponding subset definition.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::io::Cursor;
use std::io::Read;
use std::ops::RangeInclusive;

use font_types::Fixed;
use font_types::Tag;

use data_encoding::BASE64URL;
use data_encoding_macro::new_encoding;

use read_fonts::{
    collections::{IntSet, RangeSet},
    tables::ift::{
        CompatibilityId, EntryData, EntryFormatFlags, EntryMapRecord, Ift, PatchMapFormat1,
        PatchMapFormat2, IFTX_TAG, IFT_TAG,
    },
    types::{Offset32, Uint24},
    FontData, FontRead, FontRef, ReadError, TableProvider,
};

use skrifa::charmap::Charmap;

use uri_template_system::{Template, Value, Values};

// TODO(garretrieger): implement support for building and compiling mapping tables.

/// Find the set of patches which intersect the specified subset definition.
pub fn intersecting_patches(
    font: &FontRef,
    subset_definition: &SubsetDefinition,
) -> Result<Vec<PatchUri>, ReadError> {
    // TODO(garretrieger): move this function to a struct so we can optionally store
    //  indexes or other data to accelerate intersection.
    let mut result: Vec<PatchUri> = vec![];

    for (tag, table) in IftTableTag::tables_in(font) {
        add_intersecting_patches(font, tag, &table, subset_definition, &mut result)?;
    }

    Ok(result)
}

fn add_intersecting_patches(
    font: &FontRef,
    source_table: IftTableTag,
    ift: &Ift,
    subset_definition: &SubsetDefinition,
    patches: &mut Vec<PatchUri>,
) -> Result<(), ReadError> {
    match ift {
        Ift::Format1(format_1) => add_intersecting_format1_patches(
            font,
            &source_table,
            format_1,
            &subset_definition.codepoints,
            &subset_definition.feature_tags,
            patches,
        ),
        Ift::Format2(format_2) => {
            add_intersecting_format2_patches(&source_table, format_2, subset_definition, patches)
        }
    }
}

fn add_intersecting_format1_patches(
    font: &FontRef,
    source_table: &IftTableTag,
    map: &PatchMapFormat1,
    codepoints: &IntSet<u32>,
    features: &FeatureSet,
    patches: &mut Vec<PatchUri>,
) -> Result<(), ReadError> {
    // Step 0: Top Level Field Validation
    let maxp = font.maxp()?;
    if map.glyph_count() != Uint24::new(maxp.num_glyphs() as u32) {
        return Err(ReadError::MalformedData(
            "IFT glyph count must match maxp glyph count.",
        ));
    }

    let max_entry_index = map.max_entry_index();
    let max_glyph_map_entry_index = map.max_glyph_map_entry_index();
    if max_glyph_map_entry_index > max_entry_index {
        return Err(ReadError::MalformedData(
            "max_glyph_map_entry_index() must be >= max_entry_index().",
        ));
    }

    let Ok(uri_template) = map.uri_template_as_string() else {
        return Err(ReadError::MalformedData(
            "Invalid unicode string for the uri_template.",
        ));
    };

    let encoding = PatchFormat::from_format_number(map.patch_format())?;

    // Step 1: Collect the glyph and feature map entries.
    let charmap = Charmap::new(font);
    let entries = if PatchFormat::is_invalidating_format(map.patch_format()) {
        intersect_format1_glyph_and_feature_map::<true>(&charmap, map, codepoints, features)?
    } else {
        intersect_format1_glyph_and_feature_map::<false>(&charmap, map, codepoints, features)?
    };

    // Step 2: produce final output.
    let applied_entries_start_bit_index = map.shape().applied_entries_bitmap_byte_range().start * 8;
    patches.extend(
        entries
            .into_iter()
            // Entry 0 is the entry for codepoints already in the font, so it's always considered applied and skipped.
            .filter(|(index, _)| *index > 0)
            .filter(|(index, _)| !map.is_entry_applied(*index))
            .map(|(index, subset_def)| {
                PatchUri::from_index(
                    uri_template,
                    index as u32,
                    source_table.clone(),
                    applied_entries_start_bit_index + index as usize,
                    encoding,
                    if PatchFormat::is_invalidating_format(map.patch_format()) {
                        IntersectionInfo::from_subset(
                            subset_def,
                            // For format 1 the entry index is the "order",
                            // see: https://w3c.github.io/IFT/Overview.html#font-patch-invalidations
                            index.into(),
                        )
                    } else {
                        Default::default()
                    },
                )
            }),
    );
    Ok(())
}

fn intersect_format1_glyph_and_feature_map<const RECORD_INTERSECTION: bool>(
    charmap: &Charmap,
    map: &PatchMapFormat1,
    codepoints: &IntSet<u32>,
    features: &FeatureSet,
) -> Result<BTreeMap<u16, SubsetDefinition>, ReadError> {
    let mut entries = Default::default();
    intersect_format1_glyph_map::<RECORD_INTERSECTION>(charmap, map, codepoints, &mut entries)?;
    intersect_format1_feature_map::<RECORD_INTERSECTION>(map, features, &mut entries)?;
    Ok(entries)
}

fn intersect_format1_glyph_map<const RECORD_INTERSECTION: bool>(
    charmap: &Charmap,
    map: &PatchMapFormat1,
    codepoints: &IntSet<u32>,
    entries: &mut BTreeMap<u16, SubsetDefinition>,
) -> Result<(), ReadError> {
    if codepoints.is_inverted() {
        // TODO(garretrieger): consider invoking this path if codepoints set is above a size threshold
        //                     relative to the fonts cmap.
        let cp_gids = charmap
            .mappings()
            .filter(|(cp, _)| codepoints.contains(*cp))
            .map(|(cp, gid)| (cp, gid.to_u32()));
        return intersect_format1_glyph_map_inner::<RECORD_INTERSECTION>(map, cp_gids, entries);
    }

    // TODO(garretrieger): since codepoints are looked up in sorted order we may be able to speed up the charmap lookup
    // (eg. walking the charmap in parallel with the codepoints, or caching the last binary search index)
    let cp_gids = codepoints
        .iter()
        .flat_map(|cp| charmap.map(cp).map(|gid| (cp, gid.to_u32())));
    intersect_format1_glyph_map_inner::<RECORD_INTERSECTION>(map, cp_gids, entries)
}

fn intersect_format1_glyph_map_inner<const RECORD_INTERSECTION: bool>(
    map: &PatchMapFormat1,
    gids: impl Iterator<Item = (u32, u32)>,
    entries: &mut BTreeMap<u16, SubsetDefinition>,
) -> Result<(), ReadError> {
    let glyph_map = map.glyph_map()?;
    let first_gid = glyph_map.first_mapped_glyph() as u32;
    let max_glyph_map_entry_index = map.max_glyph_map_entry_index();

    for (cp, gid) in gids {
        let entry_index = if gid < first_gid {
            0
        } else {
            glyph_map
                .entry_index()
                // TODO(garretrieger): this branches to determine item size on each individual lookup, would
                //                     likely be faster if we bypassed that since all items have the same length.
                .get((gid - first_gid) as usize)?
                .get()
        };

        if entry_index > max_glyph_map_entry_index {
            continue;
        }

        let e = entries.entry(entry_index);
        let subset = e.or_default();
        if RECORD_INTERSECTION {
            subset.codepoints.insert(cp);
        }
    }

    Ok(())
}

fn intersect_format1_feature_map<const RECORD_INTERSECTION: bool>(
    map: &PatchMapFormat1,
    features: &FeatureSet,
    entries: &mut BTreeMap<u16, SubsetDefinition>,
) -> Result<(), ReadError> {
    let Some(feature_map) = map.feature_map() else {
        return Ok(());
    };
    let feature_map = feature_map?;

    let max_entry_index = map.max_entry_index();
    let max_glyph_map_entry_index = map.max_glyph_map_entry_index();
    let field_width = if max_entry_index < 256 { 1u16 } else { 2u16 };

    // We need to check up front there is enough data for all of the listed entry records, this
    // isn't checked by the read_fonts generated code. Specification requires the operation to fail
    // up front if the data is too short.
    if feature_map.entry_records_size(max_entry_index)? > feature_map.entry_map_data().len() {
        return Err(ReadError::OutOfBounds);
    }

    let mut maybe_tag_it = match features {
        FeatureSet::All => None,
        FeatureSet::Set(f) => Some(f.iter().peekable()),
    };
    let mut record_it = feature_map.feature_records().iter().peekable();

    let mut cumulative_entry_map_count = 0;
    let mut largest_tag: Option<Tag> = None;
    loop {
        let record = if let Some(tag_it) = &mut maybe_tag_it {
            let Some((tag, record)) = tag_it.peek().cloned().zip(record_it.peek().cloned()) else {
                break;
            };
            let record = record?;

            if *tag > record.feature_tag() {
                cumulative_entry_map_count += record.entry_map_count().get();
                record_it.next();
                continue;
            }

            if let Some(largest_tag) = largest_tag {
                if *tag <= largest_tag {
                    // Out of order or duplicate tag, skip this record.
                    tag_it.next();
                    continue;
                }
            }

            largest_tag = Some(*tag);

            if *tag < record.feature_tag() {
                tag_it.next();
                continue;
            }

            let Some(record) = record_it.next() else {
                break;
            };
            record?
        } else {
            // Specialization where the target set matches all feature records.
            let Some(record) = record_it.next() else {
                break;
            };
            let record = record?;

            if let Some(largest_tag) = largest_tag {
                if record.feature_tag() <= largest_tag {
                    // Out of order or duplicate tag, skip this record.
                    cumulative_entry_map_count += record.entry_map_count().get();
                    continue;
                }
            }

            largest_tag = Some(record.feature_tag());
            record
        };

        let entry_count = record.entry_map_count().get();

        for i in 0..entry_count {
            let index = i + cumulative_entry_map_count;
            let byte_index = (index * field_width * 2) as usize;
            let data = FontData::new(&feature_map.entry_map_data()[byte_index..]);
            let mapped_entry_index = record.first_new_entry_index().get() + i;
            let entry_record = EntryMapRecord::read(data, max_entry_index)?;
            let first = entry_record.first_entry_index().get();
            let last = entry_record.last_entry_index().get();
            if first > last
                || first > max_glyph_map_entry_index
                || last > max_glyph_map_entry_index
                || mapped_entry_index <= max_glyph_map_entry_index
                || mapped_entry_index > max_entry_index
            {
                // Invalid, continue on
                continue;
            }

            // If any entries exist which intersect the range of this record add all of their subset defs
            // to the new entry.
            merge_intersecting_entries::<RECORD_INTERSECTION>(
                first..=last,
                mapped_entry_index,
                record.feature_tag(),
                entries,
            );
        }

        cumulative_entry_map_count += entry_count;
    }

    Ok(())
}

fn merge_intersecting_entries<const RECORD_INTERSECTION: bool>(
    intersection: RangeInclusive<u16>,
    mapped_entry_index: u16,
    mapped_tag: Tag,
    entries: &mut BTreeMap<u16, SubsetDefinition>,
) {
    let mut range = entries.range(intersection).peekable();
    let merged_subset_def = if range.peek().is_some() {
        let mut merged_subset_def = SubsetDefinition::default();
        if RECORD_INTERSECTION {
            range.for_each(|(_, subset_def)| {
                merged_subset_def.union(subset_def);
            });
            merged_subset_def
                .feature_tags
                .extend([mapped_tag].iter().copied());
        }
        Some(merged_subset_def)
    } else {
        None
    };
    if let Some(merged_subset_def) = merged_subset_def {
        entries
            .entry(mapped_entry_index)
            .or_default()
            .union(&merged_subset_def);
    }
}

struct EntryIntersectionCache<'a> {
    entries: &'a [Entry],
    cache: HashMap<usize, bool>,
}

impl EntryIntersectionCache<'_> {
    fn intersects(&mut self, index: usize, subset_definition: &SubsetDefinition) -> bool {
        if let Some(result) = self.cache.get(&index) {
            return *result;
        }

        let Some(entry) = self.entries.get(index) else {
            return false;
        };

        let result = self.compute_intersection(entry, subset_definition);
        self.cache.insert(index, result);
        result
    }

    fn compute_intersection(
        &mut self,
        entry: &Entry,
        subset_definition: &SubsetDefinition,
    ) -> bool {
        // See: https://w3c.github.io/IFT/Overview.html#abstract-opdef-check-entry-intersection
        if !entry.intersects(subset_definition) {
            return false;
        }

        if entry.child_indices.is_empty() {
            return true;
        }

        if entry.conjunctive_child_match {
            self.all_children_intersect(entry, subset_definition)
        } else {
            self.some_children_intersect(entry, subset_definition)
        }
    }

    fn all_children_intersect(
        &mut self,
        entry: &Entry,
        subset_definition: &SubsetDefinition,
    ) -> bool {
        for child_index in entry.child_indices.iter() {
            if !self.intersects(*child_index, subset_definition) {
                return false;
            }
        }
        true
    }

    fn some_children_intersect(
        &mut self,
        entry: &Entry,
        subset_definition: &SubsetDefinition,
    ) -> bool {
        for child_index in entry.child_indices.iter() {
            if self.intersects(*child_index, subset_definition) {
                return true;
            }
        }
        false
    }
}

fn add_intersecting_format2_patches(
    source_table: &IftTableTag,
    map: &PatchMapFormat2,
    subset_definition: &SubsetDefinition,
    patches: &mut Vec<PatchUri>,
) -> Result<(), ReadError> {
    let entries = decode_format2_entries(source_table, map)?;

    // Caches the result of intersection check for an entry index.
    let mut entry_intersection_cache = EntryIntersectionCache {
        entries: &entries,
        cache: Default::default(),
    };

    for (order, e) in entries.iter().enumerate() {
        if e.ignored {
            continue;
        }

        if !entry_intersection_cache.intersects(order, subset_definition) {
            continue;
        }

        let mut uri = e.uri.clone();
        if uri.encoding().is_invalidating() {
            // for invalidating keyed patches we need to record information about intersection size to use later
            // for patch selection.
            uri.intersection_info = IntersectionInfo::from_subset(
                e.subset_definition.intersection(subset_definition),
                order,
            );
        }

        patches.push(uri)
    }

    Ok(())
}

fn decode_format2_entries(
    source_table: &IftTableTag,
    map: &PatchMapFormat2,
) -> Result<Vec<Entry>, ReadError> {
    let uri_template = map.uri_template_as_string()?;
    let entries_data = map.entries()?.entry_data();
    let default_encoding = PatchFormat::from_format_number(map.default_patch_format())?;

    let mut entry_count = map.entry_count().to_u32();
    let mut entries_data = FontData::new(entries_data);
    let mut entries: Vec<Entry> = vec![];

    let mut entry_start_byte = map.entries_offset().to_u32() as usize;

    let mut id_string_data = map
        .entry_id_string_data()
        .transpose()?
        .map(|table| table.id_data())
        .map(Cursor::new);
    while entry_count > 0 {
        let consumed_bytes;
        (entries_data, consumed_bytes) = decode_format2_entry(
            entries_data,
            entry_start_byte,
            source_table,
            uri_template,
            &default_encoding,
            &mut id_string_data,
            &mut entries,
        )?;
        entry_start_byte += consumed_bytes;
        entry_count -= 1;
    }

    Ok(entries)
}

fn decode_format2_entry<'a>(
    data: FontData<'a>,
    data_start_index: usize,
    source_table: &IftTableTag,
    uri_template: &str,
    default_encoding: &PatchFormat,
    id_string_data: &mut Option<Cursor<&[u8]>>,
    entries: &mut Vec<Entry>,
) -> Result<(FontData<'a>, usize), ReadError> {
    let entry_data = EntryData::read(
        data,
        Offset32::new(if id_string_data.is_none() { 0 } else { 1 }),
    )?;

    // Record the index of the bit which when set causes this entry to be ignored.
    // See: https://w3c.github.io/IFT/Overview.html#mapping-entry-formatflags
    let ignored_bit_index = (data_start_index * 8) + 6;
    let mut entry = Entry::new(
        uri_template,
        source_table,
        ignored_bit_index,
        default_encoding,
    );

    // Features
    if let Some(features) = entry_data.feature_tags() {
        entry
            .subset_definition
            .feature_tags
            .extend(features.iter().map(|t| t.get()));
    }

    // Copy indices
    if let (Some(child_indices), Some(match_mode)) = (
        entry_data.child_indices(),
        entry_data.match_mode_and_count(),
    ) {
        let max_index = entries.len();
        let it = child_indices.iter().map(|v| Into::<usize>::into(v.get()));
        for i in it.clone() {
            if i >= max_index {
                return Err(ReadError::MalformedData(
                    "Child index must refer to only prior entries.",
                ));
            }
        }
        entry.child_indices = it.collect();
        entry.conjunctive_child_match = match_mode.conjunctive_match();
    }

    // Design space
    if let Some(design_space_segments) = entry_data.design_space_segments() {
        let mut ranges = HashMap::<Tag, RangeSet<Fixed>>::new();

        for dss in design_space_segments {
            if dss.start() > dss.end() {
                return Err(ReadError::MalformedData(
                    "Design space segment start > end.",
                ));
            }
            ranges
                .entry(dss.axis_tag())
                .or_default()
                .insert(dss.start()..=dss.end());
        }

        entry.subset_definition.design_space = DesignSpace::Ranges(ranges);
    }

    // Entry ID
    entry.uri.id = format2_new_entry_id(&entry_data, entries.last(), id_string_data)?;

    // Encoding
    if let Some(patch_format) = entry_data.patch_format() {
        entry.uri.encoding = PatchFormat::from_format_number(patch_format)?;
    }

    // Codepoints
    let (codepoints, remaining_data) = decode_format2_codepoints(&entry_data)?;
    if entry.subset_definition.codepoints.is_empty() {
        // as an optimization move the existing set instead of copying it in if possible.
        entry.subset_definition.codepoints = codepoints;
    } else {
        entry.subset_definition.codepoints.union(&codepoints);
    }

    // Ignored
    entry.ignored = entry_data
        .format_flags()
        .contains(EntryFormatFlags::IGNORED);

    entries.push(entry);

    let consumed_bytes = entry_data.shape().codepoint_data_byte_range().end - remaining_data.len();
    Ok((FontData::new(remaining_data), consumed_bytes))
}

fn format2_new_entry_id(
    entry_data: &EntryData,
    last_entry: Option<&Entry>,
    id_string_data: &mut Option<Cursor<&[u8]>>,
) -> Result<PatchId, ReadError> {
    let Some(id_string_data) = id_string_data else {
        let last_entry_index = last_entry
            .and_then(|e| match e.uri.id {
                PatchId::Numeric(index) => Some(index),
                _ => None,
            })
            .unwrap_or(0);
        return Ok(PatchId::Numeric(compute_format2_new_entry_index(
            entry_data,
            last_entry_index,
        )?));
    };

    let Some(id_string_length) = entry_data.entry_id_delta().map(|v| v.into_inner()) else {
        let last_id_string = last_entry
            .and_then(|e| match &e.uri.id {
                PatchId::String(id_string) => Some(id_string.clone()),
                _ => None,
            })
            .unwrap_or_default();
        return Ok(PatchId::String(last_id_string));
    };

    let mut id_string: Vec<u8> = vec![0; id_string_length as usize];
    id_string_data
        .read_exact(id_string.as_mut_slice())
        .map_err(|_| ReadError::MalformedData("ID string is out of bounds."))?;
    Ok(PatchId::String(id_string))
}

fn compute_format2_new_entry_index(
    entry_data: &EntryData,
    last_entry_index: u32,
) -> Result<u32, ReadError> {
    let new_index = (last_entry_index as i64)
        + 1
        + entry_data
            .entry_id_delta()
            .map(|v| v.into_inner() as i64)
            .unwrap_or(0);

    if new_index.is_negative() {
        return Err(ReadError::MalformedData("Negative entry id encountered."));
    }

    u32::try_from(new_index).map_err(|_| {
        ReadError::MalformedData("Entry index exceeded maximum size (unsigned 32 bit).")
    })
}

fn decode_format2_codepoints<'a>(
    entry_data: &EntryData<'a>,
) -> Result<(IntSet<u32>, &'a [u8]), ReadError> {
    let format = entry_data
        .format_flags()
        .intersection(EntryFormatFlags::CODEPOINTS_BIT_1 | EntryFormatFlags::CODEPOINTS_BIT_2);

    let codepoint_data = entry_data.codepoint_data();

    if format.bits() == 0 {
        return Ok((IntSet::<u32>::empty(), codepoint_data));
    }

    // See: https://w3c.github.io/IFT/Overview.html#abstract-opdef-interpret-format-2-patch-map-entry
    // for interpretation of codepoint bit balues.
    let codepoint_data = FontData::new(codepoint_data);
    let (bias, skipped) = if format == EntryFormatFlags::CODEPOINTS_BIT_2 {
        (codepoint_data.read_at::<u16>(0)? as u32, 2)
    } else if format == (EntryFormatFlags::CODEPOINTS_BIT_1 | EntryFormatFlags::CODEPOINTS_BIT_2) {
        (codepoint_data.read_at::<Uint24>(0)?.to_u32(), 3)
    } else {
        (0, 0)
    };

    let Some(codepoint_data) = codepoint_data.split_off(skipped) else {
        return Err(ReadError::MalformedData("Codepoints data is too short."));
    };

    let (set, remaining_data) =
        IntSet::<u32>::from_sparse_bit_set_bounded(codepoint_data.as_bytes(), bias, 0x10FFFF)
            .map_err(|_| {
                ReadError::MalformedData("Failed to decode sparse bit set data stream.")
            })?;

    Ok((set, remaining_data))
}

/// Models the encoding type for a incremental font transfer patch.
/// See: <https://w3c.github.io/IFT/Overview.html#font-patch-formats-summary>
#[derive(Clone, Eq, PartialEq, Debug, Hash, Copy)]
pub enum PatchFormat {
    TableKeyed { fully_invalidating: bool },
    GlyphKeyed,
}

impl PatchFormat {
    fn is_invalidating(&self) -> bool {
        matches!(self, PatchFormat::TableKeyed { .. })
    }

    fn is_invalidating_format(format: u8) -> bool {
        match format {
            1 | 2 => true,
            3 => false,
            _ => false,
        }
    }

    fn from_format_number(format: u8) -> Result<Self, ReadError> {
        // Based on https://w3c.github.io/IFT/Overview.html#font-patch-formats-summary
        match format {
            1 => Ok(Self::TableKeyed {
                fully_invalidating: true,
            }),
            2 => Ok(Self::TableKeyed {
                fully_invalidating: false,
            }),
            3 => Ok(Self::GlyphKeyed),
            _ => Err(ReadError::MalformedData("Invalid format number.")),
        }
    }
}

/// Id for a patch which will be subbed into a URI template. The spec allows integer or string IDs.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum PatchId {
    Numeric(u32),
    String(Vec<u8>), // TODO(garretrieger): Make this a reference?
}

/// List of possible IFT mapping table tags.
#[derive(PartialEq, Eq, Debug, Clone, Hash)]
pub(crate) enum IftTableTag {
    Ift(CompatibilityId),
    Iftx(CompatibilityId),
}

impl IftTableTag {
    pub(crate) fn tables_in<'a>(font: &'a FontRef) -> impl Iterator<Item = (IftTableTag, Ift<'a>)> {
        let ift = font
            .ift()
            .map(|t| (IftTableTag::Ift(t.compatibility_id()), t))
            .into_iter();
        let iftx = font
            .iftx()
            .map(|t| (IftTableTag::Iftx(t.compatibility_id()), t))
            .into_iter();
        ift.chain(iftx)
    }

    pub(crate) fn font_compat_id(&self, font: &FontRef) -> Result<CompatibilityId, ReadError> {
        Ok(self.mapping_table(font)?.compatibility_id())
    }

    pub(crate) fn tag(&self) -> Tag {
        match self {
            Self::Ift(_) => IFT_TAG,
            Self::Iftx(_) => IFTX_TAG,
        }
    }

    pub(crate) fn mapping_table<'a>(&self, font: &'a FontRef) -> Result<Ift<'a>, ReadError> {
        font.expect_data_for_tag(self.tag())
            .and_then(FontRead::read)
    }

    pub(crate) fn expected_compat_id(&self) -> &CompatibilityId {
        match self {
            Self::Ift(cid) | Self::Iftx(cid) => cid,
        }
    }
}

/// Stores the information needed to create the URI which points to and incremental font transfer patch.
///
/// Stores a template and the arguments used to instantiate it. See:
/// <https://w3c.github.io/IFT/Overview.html#uri-templates> for details on the template format.
///
/// The input to the template expansion can be either a numeric index or a string id. Currently only
/// the numeric index is supported.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct PatchUri {
    template: String, // TODO(garretrieger): Make this a reference?
    id: PatchId,
    encoding: PatchFormat,
    source_table: IftTableTag,
    application_flag_bit_index: usize,
    intersection_info: IntersectionInfo,
}

/// Stores information on the intersection which lead to the selection of this patch.
///
/// Intersection details are used later on to choose a specific patch to apply next.
/// See: <https://w3c.github.io/IFT/Overview.html#invalidating-patch-selection>
#[derive(Debug, Clone, Eq, PartialEq, Hash, Default)]
pub(crate) struct IntersectionInfo {
    intersecting_codepoints: u64,
    intersecting_layout_tags: usize,
    intersecting_design_space: BTreeMap<Tag, Fixed>,
    entry_order: usize,
}

impl PartialOrd for IntersectionInfo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IntersectionInfo {
    fn cmp(&self, other: &Self) -> Ordering {
        // See: https://w3c.github.io/IFT/Overview.html#invalidating-patch-selection
        // for information on how these are ordered.
        match self
            .intersecting_codepoints
            .cmp(&other.intersecting_codepoints)
        {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self
            .intersecting_layout_tags
            .cmp(&other.intersecting_layout_tags)
        {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self
            .intersecting_design_space
            .cmp(&other.intersecting_design_space)
        {
            Ordering::Equal => {}
            ord => return ord,
        }

        // We select the largest intersection info, and the spec requires in ties that the lowest entry order
        // is selected. So reverse the ordering of comparing entry_order.
        self.entry_order.cmp(&other.entry_order).reverse()
    }
}

/// Indicates a malformed URI template was encountered.
///
/// More info: <https://datatracker.ietf.org/doc/html/rfc6570#section-3>
#[derive(Debug, PartialEq, Eq)]
pub struct UriTemplateError;

impl std::fmt::Display for UriTemplateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Invalid URI template encountered.")
    }
}

impl std::error::Error for UriTemplateError {}

impl PatchUri {
    const BASE32HEX_NO_PADDING: data_encoding::Encoding = new_encoding! {
        symbols: "0123456789ABCDEFGHIJKLMNOPQRSTUV",
    };

    pub fn uri_string(&self) -> Result<String, UriTemplateError> {
        let (id_string, id64_string) = match &self.id {
            PatchId::Numeric(id) => {
                let id = id.to_be_bytes();
                let id = &id[Self::count_leading_zeroes(&id)..];
                (Self::BASE32HEX_NO_PADDING.encode(id), BASE64URL.encode(id))
            }
            PatchId::String(id) => (Self::BASE32HEX_NO_PADDING.encode(id), BASE64URL.encode(id)),
        };

        let template = Template::parse(&self.template).map_err(|_| UriTemplateError)?;
        let mut values = Values::default();

        let id_string_len = id_string.len();

        for (n, name) in [(1, "d1"), (2, "d2"), (3, "d3"), (4, "d4")] {
            values = values.add(
                name,
                Value::item(
                    id_string_len
                        .checked_sub(n)
                        .and_then(|index| {
                            id_string
                                .get(index..index + 1)
                                .map(|digit| digit.to_string())
                        })
                        .unwrap_or_else(|| "_".to_string()),
                ),
            );
        }

        values = values
            .add("id", Value::item(id_string))
            .add("id64", Value::item(id64_string));

        template.expand(&values).map_err(|_| UriTemplateError)
    }

    pub(crate) fn intersection_info(&self) -> IntersectionInfo {
        self.intersection_info.clone()
    }

    pub(crate) fn source_table(self) -> IftTableTag {
        self.source_table
    }

    pub(crate) fn application_flag_bit_index(&self) -> usize {
        self.application_flag_bit_index
    }

    fn count_leading_zeroes(id: &[u8]) -> usize {
        let mut leading_bytes = 0;
        for b in id {
            if *b != 0 {
                break;
            }
            leading_bytes += 1;
        }
        // Always keep at least one digit.
        leading_bytes.min(id.len() - 1)
    }

    pub fn encoding(&self) -> PatchFormat {
        self.encoding
    }

    pub fn expected_compatibility_id(&self) -> &CompatibilityId {
        self.source_table.expected_compat_id()
    }

    pub(crate) fn from_index(
        uri_template: &str,
        entry_index: u32,
        source_table: IftTableTag,
        application_flag_bit_index: usize,
        encoding: PatchFormat,
        intersection_info: IntersectionInfo,
    ) -> PatchUri {
        PatchUri {
            template: uri_template.to_string(),
            id: PatchId::Numeric(entry_index),
            source_table,
            application_flag_bit_index,
            encoding,
            intersection_info,
        }
    }
}

impl IntersectionInfo {
    fn from_subset(value: SubsetDefinition, order: usize) -> Self {
        IntersectionInfo {
            intersecting_codepoints: value.codepoints.len(),
            intersecting_layout_tags: value.feature_tags.len(),
            intersecting_design_space: Self::design_space_size(value.design_space),
            entry_order: order,
        }
    }

    fn design_space_size(value: DesignSpace) -> BTreeMap<Tag, Fixed> {
        match value {
            DesignSpace::All => Default::default(),
            DesignSpace::Ranges(value) => value
                .into_iter()
                .map(|(tag, ranges)| {
                    let total = ranges
                        .iter()
                        .map(|range| *range.end() - *range.start())
                        .fold(Fixed::ZERO, |acc, x| acc + x);

                    (tag, total)
                })
                .collect(),
        }
    }
}

/// Stores a set of features tags, can additionally represent all features.
#[derive(Debug, Clone, PartialEq)]
pub enum FeatureSet {
    Set(BTreeSet<Tag>),
    All,
}

impl Default for FeatureSet {
    fn default() -> Self {
        Self::Set(Default::default())
    }
}

impl FeatureSet {
    fn len(&self) -> usize {
        match self {
            Self::All => usize::MAX,
            Self::Set(set) => set.len(),
        }
    }

    fn extend<It>(&mut self, tags: It)
    where
        It: Iterator<Item = Tag>,
    {
        match self {
            FeatureSet::All => {}

            FeatureSet::Set(feature_set) => {
                feature_set.extend(tags);
            }
        }
    }
}

/// Stores a collection of ranges across zero or more axes.
#[derive(Debug, Clone, PartialEq)]
pub enum DesignSpace {
    Ranges(HashMap<Tag, RangeSet<Fixed>>),
    All,
}

impl Default for DesignSpace {
    fn default() -> Self {
        Self::Ranges(Default::default())
    }
}

impl DesignSpace {
    fn is_empty(&self) -> bool {
        match self {
            Self::All => false,
            Self::Ranges(ranges) => ranges.is_empty(),
        }
    }
}

/// Stores a description of a font subset over codepoints, feature tags, and design space.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SubsetDefinition {
    codepoints: IntSet<u32>,
    feature_tags: FeatureSet,
    design_space: DesignSpace,
}

impl SubsetDefinition {
    pub fn new(
        codepoints: IntSet<u32>,
        feature_tags: FeatureSet,
        design_space: DesignSpace,
    ) -> SubsetDefinition {
        SubsetDefinition {
            codepoints,
            feature_tags,
            design_space,
        }
    }

    pub fn codepoints(codepoints: IntSet<u32>) -> SubsetDefinition {
        SubsetDefinition {
            codepoints,
            feature_tags: FeatureSet::Set(Default::default()),
            design_space: Default::default(),
        }
    }

    /// Returns a SubsetDefinition which includes all things.
    pub fn all() -> SubsetDefinition {
        SubsetDefinition {
            codepoints: IntSet::all(),
            feature_tags: FeatureSet::All,
            design_space: DesignSpace::All,
        }
    }

    fn union(&mut self, other: &SubsetDefinition) {
        self.codepoints.union(&other.codepoints);

        match &other.feature_tags {
            FeatureSet::All => self.feature_tags = FeatureSet::All,
            FeatureSet::Set(set) => self.feature_tags.extend(set.iter().copied()),
        };

        match (&other.design_space, &mut self.design_space) {
            (_, DesignSpace::All) | (DesignSpace::All, _) => self.design_space = DesignSpace::All,
            (DesignSpace::Ranges(other_ranges), DesignSpace::Ranges(self_ranges)) => {
                for (tag, segments) in other_ranges.iter() {
                    self_ranges.entry(*tag).or_default().extend(segments.iter());
                }
            }
        };
    }

    fn intersection(&self, other: &Self) -> Self {
        let mut result: SubsetDefinition = self.clone();

        result.codepoints.intersect(&other.codepoints);

        match (&self.feature_tags, &other.feature_tags) {
            (FeatureSet::All, FeatureSet::Set(tags)) => {
                result.feature_tags = FeatureSet::Set(tags.clone());
            }
            (FeatureSet::Set(a), FeatureSet::Set(b)) => {
                result.feature_tags = FeatureSet::Set(a.intersection(b).copied().collect());
            }
            // In these cases result already has the correct intersection
            (FeatureSet::All, FeatureSet::All) => {}
            (FeatureSet::Set(_), FeatureSet::All) => {}
        };

        result.design_space = self.design_space_intersection(&other.design_space);

        result
    }

    fn design_space_intersection(&self, other_design_space: &DesignSpace) -> DesignSpace {
        match (&self.design_space, other_design_space) {
            (DesignSpace::All, DesignSpace::All) => DesignSpace::All,
            (DesignSpace::All, DesignSpace::Ranges(ranges)) => DesignSpace::Ranges(ranges.clone()),
            (DesignSpace::Ranges(ranges), DesignSpace::All) => DesignSpace::Ranges(ranges.clone()),
            (DesignSpace::Ranges(self_ranges), DesignSpace::Ranges(other_ranges)) => {
                let mut result: HashMap<Tag, RangeSet<Fixed>> = Default::default();
                for (tag, input_segments) in other_ranges {
                    let Some(entry_segments) = self_ranges.get(tag) else {
                        continue;
                    };

                    let ranges: RangeSet<Fixed> =
                        input_segments.intersection(entry_segments).collect();
                    if !ranges.is_empty() {
                        result.insert(*tag, ranges);
                    }
                }

                DesignSpace::Ranges(result)
            }
        }
    }
}

/// Stores a materialized version of an IFT patchmap entry.
///
/// See: <https://w3c.github.io/IFT/Overview.html#patch-map-dfn>
#[derive(Debug, Clone, PartialEq)]
struct Entry {
    // Key
    subset_definition: SubsetDefinition,
    child_indices: Vec<usize>,
    conjunctive_child_match: bool,
    ignored: bool,

    // Value
    uri: PatchUri,
}

impl Entry {
    fn new(
        template: &str,
        source_table: &IftTableTag,
        application_flag_bit_index: usize,
        default_encoding: &PatchFormat,
    ) -> Entry {
        Entry {
            subset_definition: SubsetDefinition {
                codepoints: IntSet::empty(),
                feature_tags: Default::default(),
                design_space: Default::default(),
            },

            child_indices: Default::default(),
            conjunctive_child_match: false,
            ignored: false,

            uri: PatchUri::from_index(
                template,
                0,
                source_table.clone(),
                application_flag_bit_index,
                *default_encoding,
                Default::default(),
            ),
        }
    }

    fn intersects(&self, subset_definition: &SubsetDefinition) -> bool {
        // Intersection defined here: https://w3c.github.io/IFT/Overview.html#abstract-opdef-check-entry-intersection
        let codepoints_intersects = self.subset_definition.codepoints.is_empty()
            || self
                .subset_definition
                .codepoints
                .intersects_set(&subset_definition.codepoints);
        if !codepoints_intersects {
            return false;
        }

        let features_intersects = match &self.subset_definition.feature_tags {
            FeatureSet::All => subset_definition.feature_tags.len() > 0,
            FeatureSet::Set(set) => match &subset_definition.feature_tags {
                FeatureSet::All => true,
                FeatureSet::Set(other) => {
                    set.is_empty() || set.intersection(other).next().is_some()
                }
            },
        };

        if !features_intersects {
            return false;
        }

        match &self.subset_definition.design_space {
            DesignSpace::All => !subset_definition.design_space.is_empty(),
            DesignSpace::Ranges(entry_ranges) => match &subset_definition.design_space {
                DesignSpace::All => true,
                DesignSpace::Ranges(other_ranges) => {
                    entry_ranges.is_empty()
                        || Self::design_space_intersects(entry_ranges, other_ranges)
                }
            },
        }
    }

    fn design_space_intersects(
        a: &HashMap<Tag, RangeSet<Fixed>>,
        b: &HashMap<Tag, RangeSet<Fixed>>,
    ) -> bool {
        for (tag, a_segments) in a {
            let Some(b_segments) = b.get(tag) else {
                continue;
            };

            if a_segments.intersection(b_segments).next().is_some() {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use font_test_data as test_data;
    use font_test_data::ift::{
        child_indices_format2, codepoints_only_format2, custom_ids_format2, feature_map_format1,
        features_and_design_space_format2, simple_format1, string_ids_format2, u16_entries_format1,
    };
    use read_fonts::tables::ift::{IFTX_TAG, IFT_TAG};
    use read_fonts::types::Int24;
    use read_fonts::FontRef;
    use write_fonts::FontBuilder;

    impl FeatureSet {
        fn from<const N: usize>(tags: [Tag; N]) -> FeatureSet {
            FeatureSet::Set(BTreeSet::<Tag>::from(tags))
        }
    }

    impl DesignSpace {
        fn from<const N: usize>(design_space: [(Tag, RangeSet<Fixed>); N]) -> DesignSpace {
            DesignSpace::Ranges(design_space.into_iter().collect())
        }
    }

    impl IntersectionInfo {
        pub(crate) fn new(codepoints: u64, features: usize, order: usize) -> Self {
            IntersectionInfo {
                intersecting_codepoints: codepoints,
                intersecting_layout_tags: features,
                intersecting_design_space: Default::default(),
                entry_order: order,
            }
        }

        pub(crate) fn from_design_space<const N: usize>(
            codepoints: u64,
            features: usize,
            design_space: [(Tag, Fixed); N],
            order: usize,
        ) -> Self {
            IntersectionInfo {
                intersecting_codepoints: codepoints,
                intersecting_layout_tags: features,
                intersecting_design_space: BTreeMap::from(design_space),
                entry_order: order,
            }
        }
    }

    impl PatchUri {
        fn from_string(
            uri_template: &str,
            entry_id: &str,
            source_table: &IftTableTag,
            application_flag_bit_index: usize,
            encoding: PatchFormat,
        ) -> PatchUri {
            PatchUri {
                template: uri_template.to_string(),
                id: PatchId::String(entry_id.as_bytes().to_vec()),
                source_table: source_table.clone(),
                application_flag_bit_index,
                encoding,
                intersection_info: Default::default(),
            }
        }
    }

    fn compat_id() -> CompatibilityId {
        CompatibilityId::from_u32s([1, 2, 3, 4])
    }

    fn create_ift_font(font: FontRef, ift: Option<&[u8]>, iftx: Option<&[u8]>) -> Vec<u8> {
        let mut builder = FontBuilder::default();

        if let Some(bytes) = ift {
            builder.add_raw(IFT_TAG, bytes);
        }

        if let Some(bytes) = iftx {
            builder.add_raw(IFTX_TAG, bytes);
        }

        builder.copy_missing_tables(font);
        builder.build()
    }

    // Format 1 tests:
    // TODO(garretrieger): test w/ multi codepoints mapping to the same glyph.
    // TODO(garretrieger): test w/ IFT + IFTX both populated tables.
    // TODO(garretrieger): test which has entry that has empty codepoint array.
    // TODO(garretrieger): test with format 1 that has max entry = 0.
    // TODO(garretrieger): font with no maxp.
    // TODO(garretrieger): font with MAXP and maxp.
    // TODO(garretrieger): test for design space union of SubsetDefinition.
    // TODO(garretrieger): fuzzer to check consistency vs intersecting "*" subset def.

    #[derive(Copy, Clone)]
    struct ExpectedEntry {
        index: u32,
        application_bit_index: usize,
    }

    fn f1(index: u32) -> ExpectedEntry {
        ExpectedEntry {
            index,
            application_bit_index: (index as usize) + 36 * 8,
        }
    }

    fn f2(index: u32, entry_start: usize) -> ExpectedEntry {
        ExpectedEntry {
            index,
            application_bit_index: entry_start * 8 + 6,
        }
    }

    fn test_intersection<const M: usize, const N: usize, const O: usize>(
        font: &FontRef,
        codepoints: [u32; M],
        tags: [Tag; N],
        expected_entries: [ExpectedEntry; O],
    ) {
        test_design_space_intersection(
            font,
            codepoints,
            FeatureSet::Set(BTreeSet::<Tag>::from(tags)),
            Default::default(),
            expected_entries,
        )
    }

    fn test_design_space_intersection<const M: usize, const P: usize>(
        font: &FontRef,
        codepoints: [u32; M],
        tags: FeatureSet,
        design_space: DesignSpace,
        expected_entries: [ExpectedEntry; P],
    ) {
        let patches = intersecting_patches(
            font,
            &SubsetDefinition::new(IntSet::from(codepoints), tags, design_space),
        )
        .unwrap();

        let expected: Vec<PatchUri> = expected_entries
            .iter()
            .map(
                |ExpectedEntry {
                     index,
                     application_bit_index,
                 }| {
                    PatchUri::from_index(
                        "ABCDEFɤ",
                        *index,
                        IftTableTag::Ift(compat_id()),
                        *application_bit_index,
                        PatchFormat::GlyphKeyed,
                        Default::default(),
                    )
                },
            )
            .collect();

        assert_eq!(patches, expected);
    }

    fn test_intersection_with_all<const M: usize, const N: usize>(
        font: &FontRef,
        tags: [Tag; M],
        expected_entries: [ExpectedEntry; N],
    ) {
        let patches = intersecting_patches(
            font,
            &SubsetDefinition::new(
                IntSet::<u32>::all(),
                FeatureSet::from(tags),
                Default::default(),
            ),
        )
        .unwrap();

        let expected: Vec<PatchUri> = expected_entries
            .iter()
            .map(
                |ExpectedEntry {
                     index,
                     application_bit_index,
                 }| {
                    PatchUri::from_index(
                        "ABCDEFɤ",
                        *index,
                        IftTableTag::Ift(compat_id()),
                        *application_bit_index,
                        PatchFormat::GlyphKeyed,
                        Default::default(),
                    )
                },
            )
            .collect();

        assert_eq!(expected, patches);
    }

    fn check_uri_template_substitution(template: &str, value: u32, expected: &str) {
        assert_eq!(
            PatchUri::from_index(
                template,
                value,
                IftTableTag::Ift(Default::default()),
                0,
                PatchFormat::GlyphKeyed,
                Default::default(),
            )
            .uri_string()
            .unwrap(),
            expected,
        );
    }

    fn check_invalid_uri_template_substitution(template: &str, value: u32) {
        assert_eq!(
            PatchUri::from_index(
                template,
                value,
                IftTableTag::Ift(Default::default()),
                0,
                PatchFormat::GlyphKeyed,
                Default::default(),
            )
            .uri_string(),
            Err(UriTemplateError)
        );
    }

    fn check_string_uri_template_substitution(template: &str, value: &str, expected: &str) {
        assert_eq!(
            PatchUri::from_string(
                template,
                value,
                &IftTableTag::Ift(Default::default()),
                0,
                PatchFormat::GlyphKeyed,
            )
            .uri_string()
            .unwrap(),
            expected,
        );
    }

    #[test]
    fn uri_template_substitution() {
        // These test cases are used in other tests.
        check_uri_template_substitution("//foo.bar/{id}", 1, "//foo.bar/04");
        check_uri_template_substitution("//foo.bar/{id}", 2, "//foo.bar/08");
        check_uri_template_substitution("//foo.bar/{id}", 3, "//foo.bar/0C");
        check_uri_template_substitution("//foo.bar/{id}", 4, "//foo.bar/0G");
        check_uri_template_substitution("//foo.bar/{id}", 5, "//foo.bar/0K");

        // These test cases are from specification:
        // https://w3c.github.io/IFT/Overview.html#uri-templates
        check_uri_template_substitution("//foo.bar/{id}", 0, "//foo.bar/00");
        check_uri_template_substitution("//foo.bar/{id}", 123, "//foo.bar/FC");
        check_uri_template_substitution("//foo.bar{/d1,d2,id}", 478, "//foo.bar/0/F/07F0");
        check_uri_template_substitution("//foo.bar{/d1,d2,d3,id}", 123, "//foo.bar/C/F/_/FC");

        check_string_uri_template_substitution(
            "//foo.bar{/d1,d2,d3,id}",
            "baz",
            "//foo.bar/K/N/G/C9GNK",
        );
        check_string_uri_template_substitution(
            "//foo.bar{/d1,d2,d3,id}",
            "z",
            "//foo.bar/8/F/_/F8",
        );
        check_string_uri_template_substitution(
            "//foo.bar{/d1,d2,d3,id}",
            "àbc",
            "//foo.bar/O/O/4/OEG64OO",
        );

        check_uri_template_substitution("//foo.bar/{id64}", 0, "//foo.bar/AA%3D%3D");
        check_uri_template_substitution("//foo.bar/{id64}", 14_000_000, "//foo.bar/1Z-A");
        check_uri_template_substitution("//foo.bar/{id64}", 17_000_000, "//foo.bar/AQNmQA%3D%3D");

        check_string_uri_template_substitution("//foo.bar{/id64}", "àbc", "//foo.bar/w6BiYw%3D%3D");
        check_string_uri_template_substitution("//foo.bar/{+id64}", "àbcd", "//foo.bar/w6BiY2Q=");
    }

    #[test]
    fn invalid_uri_templates() {
        check_invalid_uri_template_substitution("//foo.bar/{i~}", 1); // non-alpha/digit variable name
        check_invalid_uri_template_substitution("  {  ݤ}", 1); // non-ascii variable name
    }

    #[test]
    fn format_1_patch_map_u8_entries() {
        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&simple_format1()),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        test_intersection(&font, [], [], []);
        test_intersection(&font, [0x123], [], []); // 0x123 is not in the mapping
        test_intersection(&font, [0x13], [], []); // 0x13 maps to entry 0
        test_intersection(&font, [0x12], [], []); // 0x12 maps to entry 1 which is applied
        test_intersection(&font, [0x11], [], [f1(2)]); // 0x11 maps to entry 2
        test_intersection(&font, [0x11, 0x12, 0x123], [], [f1(2)]);

        test_intersection_with_all(&font, [], [f1(2)]);
    }

    #[test]
    fn format_1_patch_map_bad_entry_index() {
        let mut data = simple_format1();
        data.write_at("entry_index[1]", 3u8);

        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&data),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        test_intersection(&font, [0x11], [], []);
    }

    #[test]
    fn format_1_patch_map_glyph_map_too_short() {
        let data: &[u8] = &simple_format1();
        let data = &data[..data.len() - 1];

        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(data),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        assert!(intersecting_patches(
            &font,
            &SubsetDefinition::new(
                IntSet::from([0x123]),
                FeatureSet::from([]),
                Default::default(),
            ),
        )
        .is_err());
    }

    #[test]
    fn format_1_patch_map_bad_glyph_count() {
        let font_bytes = create_ift_font(
            FontRef::new(test_data::CMAP12_FONT1).unwrap(),
            Some(&simple_format1()),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        assert!(intersecting_patches(
            &font,
            &SubsetDefinition::new(
                IntSet::from([0x123]),
                FeatureSet::from([]),
                Default::default(),
            ),
        )
        .is_err());
    }

    #[test]
    fn format_1_patch_map_bad_max_entry() {
        let mut data = simple_format1();
        data.write_at("max_glyph_map_entry_id", 3u16);

        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&data),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        assert!(intersecting_patches(
            &font,
            &SubsetDefinition::new(
                IntSet::from([0x123]),
                FeatureSet::from([]),
                Default::default(),
            ),
        )
        .is_err());
    }

    #[test]
    fn format_1_patch_map_bad_uri_template() {
        let mut data = simple_format1();
        data.write_at("uri_template[0]", 0x80u8);
        data.write_at("uri_template[1]", 0x81u8);

        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&data),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        assert!(intersecting_patches(
            &font,
            &SubsetDefinition::new(
                IntSet::from([0x123]),
                FeatureSet::from([]),
                Default::default(),
            )
        )
        .is_err());
    }

    #[test]
    fn format_1_patch_map_bad_encoding_number() {
        let mut data = simple_format1();
        data.write_at("patch_format", 0x12u8);

        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&data),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        assert!(intersecting_patches(
            &font,
            &SubsetDefinition::new(
                IntSet::from([0x123]),
                FeatureSet::from([]),
                Default::default()
            )
        )
        .is_err());
    }

    #[test]
    fn format_1_patch_map_u16_entries() {
        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&u16_entries_format1()),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        test_intersection(&font, [], [], []);
        test_intersection(&font, [0x11], [], []);
        test_intersection(&font, [0x12], [], [f1(0x50)]);
        test_intersection(&font, [0x13, 0x15], [], [f1(0x51), f1(0x12c)]);

        test_intersection_with_all(&font, [], [f1(0x50), f1(0x51), f1(0x12c)]);
    }

    #[test]
    fn format_1_patch_map_u16_entries_with_feature_mapping() {
        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&feature_map_format1()),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        test_intersection(&font, [], [], []);
        test_intersection(
            &font,
            [],
            [Tag::new(b"liga"), Tag::new(b"dlig"), Tag::new(b"null")],
            [],
        );
        test_intersection(&font, [0x12], [], [f1(0x50)]);
        test_intersection(&font, [0x12], [Tag::new(b"liga")], [f1(0x50), f1(0x180)]);
        test_intersection(
            &font,
            [0x13, 0x14],
            [Tag::new(b"liga")],
            [f1(0x51), f1(0x12c), f1(0x180), f1(0x181)],
        );
        test_intersection(
            &font,
            [0x13, 0x14],
            [Tag::new(b"dlig")],
            [f1(0x51), f1(0x12c), f1(0x190)],
        );
        test_intersection(
            &font,
            [0x13, 0x14],
            [Tag::new(b"dlig"), Tag::new(b"liga")],
            [f1(0x51), f1(0x12c), f1(0x180), f1(0x181), f1(0x190)],
        );
        test_intersection(&font, [0x11], [Tag::new(b"null")], [f1(0x12D)]);
        test_intersection(&font, [0x15], [Tag::new(b"liga")], [f1(0x181)]);

        test_intersection_with_all(&font, [], [f1(0x50), f1(0x51), f1(0x12c)]);
        test_intersection_with_all(
            &font,
            [Tag::new(b"liga")],
            [f1(0x50), f1(0x51), f1(0x12c), f1(0x180), f1(0x181)],
        );
        test_intersection_with_all(
            &font,
            [Tag::new(b"dlig")],
            [f1(0x50), f1(0x51), f1(0x12c), f1(0x190)],
        );
    }

    #[test]
    fn format_1_patch_map_all_features() {
        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&feature_map_format1()),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        test_design_space_intersection(
            &font,
            [0x13],
            FeatureSet::from([Tag::new(b"dlig"), Tag::new(b"liga")]),
            Default::default(),
            [f1(0x51), f1(0x180), f1(0x190)],
        );

        test_design_space_intersection(
            &font,
            [0x13],
            FeatureSet::All,
            Default::default(),
            [f1(0x51), f1(0x180), f1(0x190)],
        );
    }

    #[test]
    fn format_1_patch_map_all_features_skips_unsorted() {
        let mut data = feature_map_format1();
        data.write_at("FeatureRecord[0]", Tag::new(b"liga"));
        data.write_at("FeatureRecord[1]", Tag::new(b"dlig"));

        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&data),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        test_design_space_intersection(
            &font,
            [0x13, 0x14],
            FeatureSet::from([Tag::new(b"dlig"), Tag::new(b"liga"), Tag::new(b"null")]),
            Default::default(),
            [f1(0x51), f1(0x12c), f1(0x190)],
        );

        test_design_space_intersection(
            &font,
            [0x13, 0x14],
            FeatureSet::All,
            Default::default(),
            [f1(0x51), f1(0x12c), f1(0x190)],
        );
    }

    fn patch_with_intersection(
        applied_entries_start: usize,
        index: u32,
        intersection_info: IntersectionInfo,
    ) -> PatchUri {
        PatchUri::from_index(
            "ABCDEFɤ",
            index,
            IftTableTag::Ift(compat_id()),
            applied_entries_start + index as usize,
            PatchFormat::TableKeyed {
                fully_invalidating: true,
            },
            intersection_info,
        )
    }

    #[test]
    fn format_1_patch_map_intersection_info() {
        let mut map = feature_map_format1();
        map.write_at("patch_format", 1u8);
        map.write_at("gid5_entry", 299u16);
        map.write_at("gid6_entry", 300u16);
        map.write_at("applied_entries_296", 0u8);
        let applied_entries_start = map.offset_for("applied_entries") * 8;
        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&map),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        // case 1 - only codepoints
        let patches = intersecting_patches(
            &font,
            &SubsetDefinition::new(IntSet::from([0x14]), Default::default(), Default::default()),
        )
        .unwrap();
        assert_eq!(
            patches,
            vec![patch_with_intersection(
                applied_entries_start,
                300,
                IntersectionInfo::new(1, 0, 300),
            ),]
        );

        // case 2 - only codepoints
        let patches = intersecting_patches(
            &font,
            &SubsetDefinition::new(
                IntSet::from([0x14, 0x15, 0x16]),
                Default::default(),
                Default::default(),
            ),
        )
        .unwrap();
        assert_eq!(
            patches,
            vec![
                patch_with_intersection(
                    applied_entries_start,
                    299,
                    IntersectionInfo::new(1, 0, 299),
                ),
                patch_with_intersection(
                    applied_entries_start,
                    300,
                    IntersectionInfo::new(2, 0, 300),
                ),
            ]
        );

        // case 3 - features (w/ intersection)
        let patches = intersecting_patches(
            &font,
            &SubsetDefinition::new(
                IntSet::from([0x14, 0x15, 0x16]),
                FeatureSet::from([Tag::new(b"dlig"), Tag::new(b"liga")]),
                Default::default(),
            ),
        )
        .unwrap();
        assert_eq!(
            patches,
            vec![
                patch_with_intersection(
                    applied_entries_start,
                    299,
                    IntersectionInfo::new(1, 0, 299),
                ),
                patch_with_intersection(
                    applied_entries_start,
                    300,
                    IntersectionInfo::new(2, 0, 300),
                ),
                patch_with_intersection(
                    applied_entries_start,
                    385,
                    IntersectionInfo::new(3, 1, 385),
                ),
            ]
        );

        // case 4 - features (w/o intersection)
        let patches = intersecting_patches(
            &font,
            &SubsetDefinition::new(
                IntSet::from([0x14, 0x15, 0x16]),
                FeatureSet::from([Tag::new(b"dlig")]),
                Default::default(),
            ),
        )
        .unwrap();
        assert_eq!(
            patches,
            vec![
                patch_with_intersection(
                    applied_entries_start,
                    299,
                    IntersectionInfo::new(1, 0, 299),
                ),
                patch_with_intersection(
                    applied_entries_start,
                    300,
                    IntersectionInfo::new(2, 0, 300),
                ),
            ]
        );
    }

    #[test]
    fn format_1_patch_map_u16_entries_with_out_of_order_feature_mapping() {
        let mut data = feature_map_format1();
        data.write_at("FeatureRecord[0]", Tag::new(b"liga"));
        data.write_at("FeatureRecord[1]", Tag::new(b"dlig"));

        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&data),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        test_intersection(
            &font,
            [0x13, 0x14],
            [Tag::new(b"liga")],
            [f1(0x51), f1(0x12c), f1(0x190)],
        );
        test_intersection(
            &font,
            [0x13, 0x14],
            [Tag::new(b"dlig")],
            [f1(0x51), f1(0x12c)], // dlig is ignored since it's out of order.
        );
        test_intersection(&font, [0x11], [Tag::new(b"null")], [f1(0x12D)]);
    }

    #[test]
    fn format_1_patch_map_u16_entries_with_duplicate_feature_mapping() {
        let mut data = feature_map_format1();
        data.write_at("FeatureRecord[0]", Tag::new(b"liga"));
        data.write_at("FeatureRecord[1]", Tag::new(b"liga"));

        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&data),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        test_intersection(
            &font,
            [0x13, 0x14],
            [Tag::new(b"liga")],
            [f1(0x51), f1(0x12c), f1(0x190)],
        );
        test_intersection(&font, [0x11], [Tag::new(b"null")], [f1(0x12D)]);
    }

    #[test]
    fn format_1_patch_map_feature_map_entry_record_too_short() {
        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&feature_map_format1()[..feature_map_format1().len() - 1]),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        assert!(intersecting_patches(
            &font,
            &SubsetDefinition::new(
                IntSet::from([0x12]),
                FeatureSet::from([]),
                Default::default(),
            ),
        )
        .is_err());
        assert!(intersecting_patches(
            &font,
            &SubsetDefinition::new(
                IntSet::from([0x12]),
                FeatureSet::from([Tag::new(b"liga")]),
                Default::default(),
            )
        )
        .is_err());
        assert!(intersecting_patches(
            &font,
            &SubsetDefinition::new(
                IntSet::from([0x12]),
                FeatureSet::from([]),
                Default::default(),
            )
        )
        .is_err());
    }

    #[test]
    fn format_1_patch_map_feature_record_too_short() {
        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&feature_map_format1()[..123]),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        assert!(intersecting_patches(
            &font,
            &SubsetDefinition::new(
                IntSet::from([0x12]),
                FeatureSet::from([]),
                Default::default(),
            ),
        )
        .is_err());
        assert!(intersecting_patches(
            &font,
            &SubsetDefinition::new(
                IntSet::from([0x12]),
                FeatureSet::from([Tag::new(b"liga")]),
                Default::default(),
            )
        )
        .is_err());
        assert!(intersecting_patches(
            &font,
            &SubsetDefinition::new(
                IntSet::from([0x12]),
                FeatureSet::from([]),
                Default::default(),
            )
        )
        .is_err());
    }

    #[test]
    fn format_2_patch_map_codepoints_only() {
        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&codepoints_only_format2()),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        let e1 = f2(1, codepoints_only_format2().offset_for("entries[0]"));
        let e3 = f2(3, codepoints_only_format2().offset_for("entries[2]"));
        let e4 = f2(4, codepoints_only_format2().offset_for("entries[3]"));
        test_intersection(&font, [], [], []);
        test_intersection(&font, [0x02], [], [e1]);
        test_intersection(&font, [0x15], [], [e3]);
        test_intersection(&font, [0x07], [], [e1, e3]);
        test_intersection(&font, [80_007], [], [e4]);

        test_intersection_with_all(&font, [], [e1, e3, e4]);
    }

    #[test]
    fn format_2_patch_map_features_and_design_space() {
        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&features_and_design_space_format2()),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        let e1 = f2(
            1,
            features_and_design_space_format2().offset_for("entries[0]"),
        );
        let e2 = f2(
            2,
            features_and_design_space_format2().offset_for("entries[1]"),
        );
        let e3 = f2(
            3,
            features_and_design_space_format2().offset_for("entries[2]"),
        );

        test_intersection(&font, [], [], []);
        test_intersection(&font, [0x02], [], []);
        test_intersection(&font, [0x50], [Tag::new(b"rlig")], []);
        test_intersection(&font, [0x02], [Tag::new(b"rlig")], [e2]);

        test_design_space_intersection(
            &font,
            [0x02],
            FeatureSet::from([Tag::new(b"rlig")]),
            DesignSpace::from([(
                Tag::new(b"wdth"),
                [Fixed::from_f64(0.7)..=Fixed::from_f64(0.8)]
                    .into_iter()
                    .collect(),
            )]),
            [e2],
        );

        test_design_space_intersection(
            &font,
            [0x05],
            FeatureSet::from([Tag::new(b"smcp")]),
            DesignSpace::from([(
                Tag::new(b"wdth"),
                [Fixed::from_f64(0.7)..=Fixed::from_f64(0.8)]
                    .into_iter()
                    .collect(),
            )]),
            [e1],
        );
        test_design_space_intersection(
            &font,
            [0x05],
            FeatureSet::from([Tag::new(b"smcp")]),
            DesignSpace::from([(
                Tag::new(b"wdth"),
                [Fixed::from_f64(0.2)..=Fixed::from_f64(0.3)]
                    .into_iter()
                    .collect(),
            )]),
            [e3],
        );
        test_design_space_intersection(
            &font,
            [0x55],
            FeatureSet::from([Tag::new(b"smcp")]),
            DesignSpace::from([(
                Tag::new(b"wdth"),
                [Fixed::from_f64(0.2)..=Fixed::from_f64(0.3)]
                    .into_iter()
                    .collect(),
            )]),
            [],
        );
        test_design_space_intersection(
            &font,
            [0x05],
            FeatureSet::from([Tag::new(b"smcp")]),
            DesignSpace::from([(
                Tag::new(b"wdth"),
                [Fixed::from_f64(1.2)..=Fixed::from_f64(1.3)]
                    .into_iter()
                    .collect(),
            )]),
            [],
        );

        test_design_space_intersection(
            &font,
            [0x05],
            FeatureSet::from([Tag::new(b"smcp")]),
            DesignSpace::from([(
                Tag::new(b"wdth"),
                [
                    Fixed::from_f64(0.2)..=Fixed::from_f64(0.3),
                    Fixed::from_f64(0.7)..=Fixed::from_f64(0.8),
                ]
                .into_iter()
                .collect(),
            )]),
            [e1, e3],
        );
        test_design_space_intersection(
            &font,
            [0x05],
            FeatureSet::from([Tag::new(b"smcp")]),
            DesignSpace::from([(
                Tag::new(b"wdth"),
                [Fixed::from_f64(2.2)..=Fixed::from_f64(2.3)]
                    .into_iter()
                    .collect(),
            )]),
            [e3],
        );
        test_design_space_intersection(
            &font,
            [0x05],
            FeatureSet::from([Tag::new(b"smcp")]),
            DesignSpace::from([(
                Tag::new(b"wdth"),
                [
                    Fixed::from_f64(2.2)..=Fixed::from_f64(2.3),
                    Fixed::from_f64(1.2)..=Fixed::from_f64(1.3),
                ]
                .into_iter()
                .collect(),
            )]),
            [e3],
        );
    }

    #[test]
    fn format_2_patch_map_all_features() {
        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&features_and_design_space_format2()),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        let e1 = f2(
            1,
            features_and_design_space_format2().offset_for("entries[0]"),
        );
        let e2 = f2(
            2,
            features_and_design_space_format2().offset_for("entries[1]"),
        );
        let e3 = f2(
            3,
            features_and_design_space_format2().offset_for("entries[2]"),
        );

        test_design_space_intersection(
            &font,
            [0x06],
            FeatureSet::All,
            DesignSpace::from([(
                Tag::new(b"wdth"),
                [Fixed::from_f64(0.7)..=Fixed::from_f64(2.2)]
                    .into_iter()
                    .collect(),
            )]),
            [e1, e2, e3],
        );
    }

    #[test]
    fn format_2_patch_map_all_design_space() {
        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&features_and_design_space_format2()),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        let e1 = f2(
            1,
            features_and_design_space_format2().offset_for("entries[0]"),
        );
        let e2 = f2(
            2,
            features_and_design_space_format2().offset_for("entries[1]"),
        );
        let e3 = f2(
            3,
            features_and_design_space_format2().offset_for("entries[2]"),
        );

        test_design_space_intersection(
            &font,
            [0x05],
            FeatureSet::from([Tag::new(b"smcp")]),
            DesignSpace::All,
            [e1, e3],
        );

        test_design_space_intersection(
            &font,
            [0x05],
            FeatureSet::All,
            DesignSpace::All,
            [e1, e2, e3],
        );
    }

    #[test]
    fn format_2_patch_map_intersection_info() {
        let mut map = features_and_design_space_format2();
        map.write_at("patch_format", 1u8);

        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&map),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        // Case 1
        let patches = intersecting_patches(
            &font,
            &SubsetDefinition::new(
                IntSet::from([10, 15, 22]),
                FeatureSet::from([Tag::new(b"rlig"), Tag::new(b"liga")]),
                Default::default(),
            ),
        )
        .unwrap();
        assert_eq!(
            patches,
            vec![patch_with_intersection(
                map.offset_for("entries[1]") * 8 + 4,
                2,
                IntersectionInfo::new(2, 1, 1),
            ),]
        );

        // Case 2
        let patches = intersecting_patches(
            &font,
            &SubsetDefinition::new(
                IntSet::from([10, 15, 22]),
                FeatureSet::from([Tag::new(b"rlig"), Tag::new(b"liga"), Tag::new(b"smcp")]),
                DesignSpace::from([(
                    Tag::new(b"wght"),
                    [Fixed::from_i32(505)..=Fixed::from_i32(800)]
                        .into_iter()
                        .collect(),
                )]),
            ),
        )
        .unwrap();
        assert_eq!(
            patches,
            vec![
                patch_with_intersection(
                    map.offset_for("entries[1]") * 8 + 4,
                    2,
                    IntersectionInfo::new(2, 1, 1),
                ),
                patch_with_intersection(
                    map.offset_for("entries[2]") * 8 + 3,
                    3,
                    IntersectionInfo::from_design_space(
                        3,
                        1,
                        [(Tag::new(b"wght"), Fixed::from_i32(195))],
                        2
                    ),
                ),
            ]
        );
    }

    #[test]
    fn format_2_patch_map_invalid_child_indices() {
        let mut builder = child_indices_format2();
        builder.write_at("entries[6]_child", Uint24::new(6));

        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&builder),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        assert_eq!(
            intersecting_patches(
                &font,
                &SubsetDefinition::new(IntSet::all(), FeatureSet::from([]), Default::default()),
            ),
            Err(ReadError::MalformedData(
                "Child index must refer to only prior entries."
            ))
        );
    }

    #[test]
    fn format_2_patch_map_disjunctive_child_indices() {
        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&child_indices_format2()),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        let e3 = f2(3, child_indices_format2().offset_for("entries[2]"));
        let e5 = f2(5, child_indices_format2().offset_for("entries[4]"));
        let e6 = f2(6, child_indices_format2().offset_for("entries[5]"));
        let e7 = f2(7, child_indices_format2().offset_for("entries[6]"));
        let e8 = f2(8, child_indices_format2().offset_for("entries[7]"));
        let e9 = f2(9, child_indices_format2().offset_for("entries[8]"));
        test_intersection(&font, [], [], []);
        test_intersection(&font, [0x05], [], [e5, e7, e8]);
        test_intersection(&font, [0x65], [], []);
        test_intersection(&font, [0x05, 0x65], [], [e5, e7, e8, e9]);

        test_design_space_intersection(
            &font,
            [],
            FeatureSet::from([Tag::new(b"rlig")]),
            DesignSpace::from([(
                Tag::new(b"wght"),
                [Fixed::from_i32(500)..=Fixed::from_i32(500)]
                    .into_iter()
                    .collect(),
            )]),
            [e3, e6, e7, e8],
        );

        test_design_space_intersection(
            &font,
            [0x05],
            FeatureSet::from([Tag::new(b"rlig")]),
            DesignSpace::from([(
                Tag::new(b"wght"),
                [Fixed::from_i32(500)..=Fixed::from_i32(500)]
                    .into_iter()
                    .collect(),
            )]),
            [e3, e5, e6, e7, e8],
        );
    }

    #[test]
    fn format_2_patch_map_conjunctive_child_indices() {
        let mut builder = child_indices_format2();
        builder.write_at("entries[6]_child_count", 0b10000000u8 | 4u8);

        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&builder),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        let e2 = f2(2, child_indices_format2().offset_for("entries[1]"));
        let e3 = f2(3, child_indices_format2().offset_for("entries[2]"));
        let e4 = f2(4, child_indices_format2().offset_for("entries[3]"));
        let e5 = f2(5, child_indices_format2().offset_for("entries[4]"));
        let e6 = f2(6, child_indices_format2().offset_for("entries[5]"));
        let e7 = f2(7, child_indices_format2().offset_for("entries[6]"));
        let e8 = f2(8, child_indices_format2().offset_for("entries[7]"));
        test_intersection(&font, [0x05], [], [e5, e8]);
        test_design_space_intersection(
            &font,
            [0x05, 51],
            FeatureSet::from([Tag::new(b"liga"), Tag::new(b"rlig")]),
            DesignSpace::from([(
                Tag::new(b"wght"),
                [
                    Fixed::from_i32(75)..=Fixed::from_i32(75),
                    Fixed::from_i32(500)..=Fixed::from_i32(500),
                ]
                .into_iter()
                .collect(),
            )]),
            [e2, e3, e4, e5, e6, e7, e8],
        );
    }

    #[test]
    fn format_2_patch_map_custom_ids() {
        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&custom_ids_format2()),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        let e0 = f2(0, custom_ids_format2().offset_for("entries[0]"));
        let e6 = f2(6, custom_ids_format2().offset_for("entries[1]"));
        let e15 = f2(15, custom_ids_format2().offset_for("entries[3]"));

        test_intersection_with_all(&font, [], [e0, e6, e15]);
    }

    #[test]
    fn format_2_patch_map_custom_encoding() {
        let mut data = custom_ids_format2();
        data.write_at("entry[4] encoding", 1u8); // Tabled Keyed Full Invalidation.

        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&data),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        let patches = intersecting_patches(
            &font,
            &SubsetDefinition::new(IntSet::all(), FeatureSet::from([]), Default::default()),
        )
        .unwrap();

        let encodings: Vec<PatchFormat> = patches.into_iter().map(|uri| uri.encoding).collect();
        assert_eq!(
            encodings,
            vec![
                PatchFormat::GlyphKeyed,
                PatchFormat::GlyphKeyed,
                PatchFormat::TableKeyed {
                    fully_invalidating: true,
                },
            ]
        );
    }

    #[test]
    fn format_2_patch_map_id_strings() {
        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&string_ids_format2()),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        let patches = intersecting_patches(
            &font,
            &SubsetDefinition::new(IntSet::all(), FeatureSet::from([]), Default::default()),
        )
        .unwrap();

        let ids: Vec<PatchId> = patches.into_iter().map(|uri| uri.id).collect();
        let expected_ids = vec!["", "abc", "defg", "defg", "hij", ""];
        assert_eq!(
            ids,
            expected_ids
                .into_iter()
                .map(|s| PatchId::String(Vec::from(s)))
                .collect::<Vec<PatchId>>()
        );
    }

    #[test]
    fn format_2_patch_map_id_strings_too_short() {
        let mut data = string_ids_format2();
        data.write_at("entry[4] id length", 4u16);

        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&data),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        assert!(intersecting_patches(
            &font,
            &SubsetDefinition::new(IntSet::all(), FeatureSet::from([]), Default::default()),
        )
        .is_err());
    }

    #[test]
    fn format_2_patch_map_invalid_design_space() {
        let mut data = features_and_design_space_format2();
        data.write_at("wdth start", 0x20000u32);

        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&data),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        assert!(intersecting_patches(
            &font,
            &SubsetDefinition::new(IntSet::all(), FeatureSet::from([]), Default::default()),
        )
        .is_err());
    }

    #[test]
    fn format_2_patch_map_invalid_sparse_bit_set() {
        let data = codepoints_only_format2();
        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&data[..(data.len() - 1)]),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        assert!(intersecting_patches(
            &font,
            &SubsetDefinition::new(IntSet::all(), FeatureSet::from([]), Default::default()),
        )
        .is_err());
    }

    #[test]
    fn format_2_patch_map_negative_entry_id() {
        let mut data = custom_ids_format2();
        data.write_at("id delta", Int24::new(-2));

        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&data),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        assert!(intersecting_patches(
            &font,
            &SubsetDefinition::new(IntSet::all(), FeatureSet::from([]), Default::default()),
        )
        .is_err());
    }

    #[test]
    fn format_2_patch_map_negative_entry_id_on_ignored() {
        let mut data = custom_ids_format2();
        data.write_at("id delta - ignored entry", Int24::new(-20));

        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&data),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        assert!(intersecting_patches(
            &font,
            &SubsetDefinition::new(IntSet::all(), FeatureSet::from([]), Default::default()),
        )
        .is_err());
    }

    #[test]
    fn format_2_patch_map_entry_id_overflow() {
        let count = 511;
        let mut data = custom_ids_format2();
        data.write_at("entry_count", Uint24::new(count + 5));

        for _ in 0..count {
            data = data
                .push(0b01000100u8) // format = ID_DELTA | IGNORED
                .push(Int24::new(0x7FFFFF)); // delta = max(i24)
        }

        // at this point the second last entry id is:
        // 15 +                   # last entry id from the first 4 entries
        // count * (0x7FFFFF + 1) # sum of added deltas
        //
        // So the max delta without overflow on the last entry is:
        //
        // u32::MAX - second last entry id - 1
        //
        // The -1 is needed because the last entry implicitly includes a + 1
        let max_delta_without_overflow = (u32::MAX - ((15 + count * (0x7FFFFF + 1)) + 1)) as i32;
        data = data
            .push(0b01000100u8) // format = ID_DELTA | IGNORED
            .push_with_tag(Int24::new(max_delta_without_overflow), "last delta"); // delta

        // Check one less than max doesn't overflow
        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&data),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        assert!(intersecting_patches(
            &font,
            &SubsetDefinition::new(IntSet::all(), FeatureSet::from([]), Default::default()),
        )
        .is_ok());

        // Check one more does overflow
        data.write_at("last delta", Int24::new(max_delta_without_overflow + 1));

        let font_bytes = create_ift_font(
            FontRef::new(test_data::ift::IFT_BASE).unwrap(),
            Some(&data),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        assert!(intersecting_patches(
            &font,
            &SubsetDefinition::new(IntSet::all(), FeatureSet::from([]), Default::default()),
        )
        .is_err());
    }

    #[test]
    fn intersection_info_ordering() {
        // these are in the correct order
        let v1 = IntersectionInfo::new(5, 9, 1);
        let v2 = IntersectionInfo::new(5, 10, 2);
        let v3 = IntersectionInfo::new(5, 10, 1);
        let v4 = IntersectionInfo::new(6, 1, 10);
        let v5 = IntersectionInfo::new(6, 1, 9);

        assert_eq!(v1.cmp(&v1), Ordering::Equal);

        assert_eq!(v1.cmp(&v2), Ordering::Less);
        assert_eq!(v2.cmp(&v1), Ordering::Greater);

        assert_eq!(v2.cmp(&v3), Ordering::Less);
        assert_eq!(v3.cmp(&v2), Ordering::Greater);

        assert_eq!(v3.cmp(&v4), Ordering::Less);
        assert_eq!(v4.cmp(&v3), Ordering::Greater);

        assert_eq!(v4.cmp(&v5), Ordering::Less);
        assert_eq!(v5.cmp(&v4), Ordering::Greater);

        assert_eq!(v3.cmp(&v5), Ordering::Less);
        assert_eq!(v5.cmp(&v3), Ordering::Greater);
    }

    #[test]
    fn intersection_info_ordering_with_design_space() {
        let aaaa = Tag::new(b"aaaa");
        let bbbb = Tag::new(b"bbbb");

        // these are in the correct order
        let v1 = IntersectionInfo::from_design_space(1, 0, [(aaaa, 4i32.into())], 0);
        let v2 = IntersectionInfo::from_design_space(
            1,
            0,
            [(aaaa, 5i32.into()), (bbbb, 2i32.into())],
            0,
        );
        let v3 = IntersectionInfo::from_design_space(1, 0, [(aaaa, 6i32.into())], 0);
        let v4 = IntersectionInfo::from_design_space(
            1,
            0,
            [(aaaa, 6i32.into()), (bbbb, 2i32.into())],
            0,
        );
        let v5 = IntersectionInfo::from_design_space(1, 0, [(bbbb, 1i32.into())], 0);
        let v6 = IntersectionInfo::from_design_space(2, 0, [(aaaa, 1i32.into())], 0);

        assert_eq!(v1.cmp(&v1), Ordering::Equal);

        assert_eq!(v1.cmp(&v2), Ordering::Less);
        assert_eq!(v2.cmp(&v1), Ordering::Greater);

        assert_eq!(v2.cmp(&v3), Ordering::Less);
        assert_eq!(v3.cmp(&v2), Ordering::Greater);

        assert_eq!(v3.cmp(&v4), Ordering::Less);
        assert_eq!(v4.cmp(&v3), Ordering::Greater);

        assert_eq!(v4.cmp(&v5), Ordering::Less);
        assert_eq!(v5.cmp(&v4), Ordering::Greater);

        assert_eq!(v5.cmp(&v6), Ordering::Less);
        assert_eq!(v6.cmp(&v5), Ordering::Greater);

        assert_eq!(v3.cmp(&v5), Ordering::Less);
        assert_eq!(v5.cmp(&v3), Ordering::Greater);
    }

    #[test]
    fn entry_design_codepoints_intersection() {
        let uri = PatchUri::from_index(
            "",
            0,
            IftTableTag::Ift(CompatibilityId::from_u32s([0, 0, 0, 0])),
            0,
            PatchFormat::GlyphKeyed,
            IntersectionInfo::default(),
        );

        let s1 = SubsetDefinition::codepoints([3, 5, 7].into_iter().collect());
        let s2 = SubsetDefinition::codepoints([13, 15, 17].into_iter().collect());
        let s3 = SubsetDefinition::codepoints([7, 13].into_iter().collect());

        let e1 = Entry {
            subset_definition: s1.clone(),
            uri: uri.clone(),
            ignored: false,
            child_indices: Default::default(),
            conjunctive_child_match: Default::default(),
        };
        let e2 = Entry {
            subset_definition: Default::default(),
            uri: uri.clone(),
            ignored: false,
            child_indices: Default::default(),
            conjunctive_child_match: Default::default(),
        };

        assert!(e1.intersects(&s1));
        assert_eq!(s1.intersection(&s1), s1);

        assert!(!e1.intersects(&s2));
        assert_eq!(s1.intersection(&s2), Default::default());

        assert!(e1.intersects(&s3));
        assert_eq!(
            s1.intersection(&s3),
            SubsetDefinition::codepoints([7].into_iter().collect())
        );

        assert!(e2.intersects(&s1));
        assert_eq!(
            SubsetDefinition::default().intersection(&s1),
            Default::default()
        );
    }

    #[test]
    fn entry_design_space_intersection() {
        let uri = PatchUri::from_index(
            "",
            0,
            IftTableTag::Ift(CompatibilityId::from_u32s([0, 0, 0, 0])),
            0,
            PatchFormat::GlyphKeyed,
            IntersectionInfo::default(),
        );

        let s1 = SubsetDefinition::new(
            Default::default(),
            Default::default(),
            DesignSpace::from([
                (
                    Tag::new(b"aaaa"),
                    [Fixed::from_i32(100)..=Fixed::from_i32(200)]
                        .into_iter()
                        .collect(),
                ),
                (
                    Tag::new(b"bbbb"),
                    [
                        Fixed::from_i32(300)..=Fixed::from_i32(600),
                        Fixed::from_i32(700)..=Fixed::from_i32(900),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ]),
        );
        let s2 = SubsetDefinition::new(
            Default::default(),
            Default::default(),
            DesignSpace::from([
                (
                    Tag::new(b"bbbb"),
                    [Fixed::from_i32(100)..=Fixed::from_i32(200)]
                        .into_iter()
                        .collect(),
                ),
                (
                    Tag::new(b"cccc"),
                    [Fixed::from_i32(300)..=Fixed::from_i32(600)]
                        .into_iter()
                        .collect(),
                ),
            ]),
        );
        let s3 = SubsetDefinition::new(
            Default::default(),
            Default::default(),
            DesignSpace::from([
                (
                    Tag::new(b"bbbb"),
                    [Fixed::from_i32(500)..=Fixed::from_i32(800)]
                        .into_iter()
                        .collect(),
                ),
                (
                    Tag::new(b"cccc"),
                    [Fixed::from_i32(300)..=Fixed::from_i32(600)]
                        .into_iter()
                        .collect(),
                ),
            ]),
        );
        let s4 = SubsetDefinition::new(
            Default::default(),
            Default::default(),
            DesignSpace::from([(
                Tag::new(b"bbbb"),
                [
                    Fixed::from_i32(500)..=Fixed::from_i32(600),
                    Fixed::from_i32(700)..=Fixed::from_i32(800),
                ]
                .into_iter()
                .collect(),
            )]),
        );

        let e1 = Entry {
            subset_definition: s1.clone(),
            uri: uri.clone(),
            ignored: false,
            child_indices: Default::default(),
            conjunctive_child_match: Default::default(),
        };
        let e2 = Entry {
            subset_definition: Default::default(),
            uri: uri.clone(),
            ignored: false,
            child_indices: Default::default(),
            conjunctive_child_match: Default::default(),
        };

        assert!(e1.intersects(&s1));
        assert_eq!(s1.intersection(&s1), s1.clone());

        assert!(!e1.intersects(&s2));
        assert_eq!(s1.intersection(&s2), Default::default());

        assert!(e1.intersects(&s3));
        assert_eq!(s1.intersection(&s3), s4.clone());

        assert!(e2.intersects(&s1));
        assert_eq!(
            SubsetDefinition::default().intersection(&s1),
            SubsetDefinition::default()
        );
    }
}
