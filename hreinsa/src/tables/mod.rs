//! Table sanitization pipeline and orchestration.

pub mod cmap;
pub mod glyf;
pub mod head;
pub mod hhea;
pub mod hmtx;
pub mod loca;
pub mod maxp;
pub mod name;
pub mod os2;
pub mod post;

use std::collections::HashMap;
use font_types::Tag;
use write_fonts::FontBuilder;

use crate::container::{is_printable_tag, RawTableEntry, SfntDirectory};
use crate::context::{SanitizeContext, TableAction};
use crate::error::{MessageLevel, SanitizeError};

const CFF_TAG: Tag = Tag::new(b"CFF ");
const CFF2_TAG: Tag = Tag::new(b"CFF2");

/// Run the full table sanitization pipeline for an SFNT font.
pub fn sanitize_tables(
    data: &[u8],
    directory: &SfntDirectory,
    context: &mut dyn SanitizeContext,
) -> Result<Vec<u8>, SanitizeError> {
    let mut table_map: HashMap<Tag, RawTableEntry> = HashMap::new();
    for entry in &directory.tables {
        table_map.insert(entry.tag, *entry);
    }

    let get_table_data = |tag: Tag| -> Result<&[u8], SanitizeError> {
        let entry = table_map
            .get(&tag)
            .ok_or(SanitizeError::MissingRequiredTable(tag))?;
        let start = entry.offset as usize;
        let end = start + entry.length as usize;
        if end > data.len() {
            return Err(SanitizeError::TableOverrunsFile {
                tag,
                offset: entry.offset,
                length: entry.length,
                file_size: data.len(),
            });
        }
        Ok(&data[start..end])
    };

    // Check for glyph outline tables
    let has_glyf = table_map.contains_key(&glyf::TAG);
    let has_loca = table_map.contains_key(&loca::TAG);
    let has_cff = table_map.contains_key(&CFF_TAG) || table_map.contains_key(&CFF2_TAG);

    if !has_cff && (!has_glyf || !has_loca) {
        return Err(SanitizeError::NoGlyphData);
    }

    // 1. maxp
    let maxp_data = get_table_data(maxp::TAG)?;
    let mut maxp = maxp::MaxpState::parse(maxp_data, context)?;

    // 2. head
    let head_data = get_table_data(head::TAG)?;
    let mut head = head::HeadState::parse(head_data, context)?;

    // 3. OS/2
    let os2_data = get_table_data(os2::TAG)?;
    let os2 = os2::Os2State::parse(os2_data, &mut head, context)?;

    // 4. cmap
    let cmap_data = get_table_data(cmap::TAG)?;
    let cmap_bytes = cmap::CmapState::sanitize(cmap_data, &maxp, context)?;

    // 5. hhea
    let hhea_data = get_table_data(hhea::TAG)?;
    let hhea = hhea::HheaState::parse(hhea_data, &maxp, context)?;

    // 6. hmtx
    let hmtx_data = get_table_data(hmtx::TAG)?;
    let hmtx_bytes = hmtx::HmtxState::sanitize(hmtx_data, &hhea, &maxp, context)?;

    // 7. name
    let name_data = get_table_data(name::TAG)?;
    let name = name::NameState::parse(name_data, context)?;

    // 8. post
    let post_data = get_table_data(post::TAG)?;
    let post = post::PostState::parse(post_data, &maxp, has_cff, context)?;

    // 9 & 10. loca & glyf (if TrueType outlines are used)
    let glyf_and_loca = if has_glyf && has_loca {
        let loca_data = get_table_data(loca::TAG)?;
        let loca = loca::LocaState::parse(loca_data, &head, &maxp, context)?;

        let glyf_data = get_table_data(glyf::TAG)?;
        let glyf_bytes = glyf::GlyfState::sanitize(glyf_data, &loca, &mut maxp, context)?;

        Some((loca, glyf_bytes))
    } else {
        None
    };

    // Assemble sanitized font with FontBuilder
    let mut builder = FontBuilder::new();

    // Add required and sanitized core tables
    builder.add_raw(head::TAG, head.serialize());
    builder.add_raw(maxp::TAG, maxp.serialize());
    builder.add_raw(os2::TAG, os2.serialize());
    builder.add_raw(cmap::TAG, cmap_bytes);
    builder.add_raw(hhea::TAG, hhea.serialize());
    builder.add_raw(hmtx::TAG, hmtx_bytes);
    builder.add_raw(name::TAG, name.serialize());
    builder.add_raw(post::TAG, post.serialize());

    if let Some((loca, glyf_bytes)) = glyf_and_loca {
        builder.add_raw(loca::TAG, loca.serialize(&head));
        builder.add_raw(glyf::TAG, glyf_bytes);
    }

    // Handle remaining optional tables
    for entry in &directory.tables {
        let tag = entry.tag;
        // Skip already sanitized tables
        if [
            head::TAG,
            maxp::TAG,
            os2::TAG,
            cmap::TAG,
            hhea::TAG,
            hmtx::TAG,
            name::TAG,
            post::TAG,
            loca::TAG,
            glyf::TAG,
        ]
        .contains(&tag)
        {
            continue;
        }

        // If font has TrueType outlines and CFF is present, drop CFF per OTS spec
        if has_glyf && has_loca && (tag == CFF_TAG || tag == CFF2_TAG) {
            context.message(
                MessageLevel::Warning,
                &format!("Dropping {tag} because TrueType glyph outlines are present"),
            );
            continue;
        }

        let action = context.get_table_action(tag);
        let should_include = match action {
            TableAction::Drop => false,
            TableAction::PassThru => true,
            TableAction::Sanitize | TableAction::SanitizeSoft => true,
            TableAction::Default => {
                // Pass-through standard printable tables by default
                is_printable_tag(tag)
            }
        };

        if should_include {
            let start = entry.offset as usize;
            let end = start + entry.length as usize;
            if end <= data.len() {
                builder.add_raw(tag, &data[start..end]);
            }
        }
    }

    Ok(builder.build())
}
