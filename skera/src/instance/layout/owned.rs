//! Preserve borrowed layout details when converting to write-fonts tables.
use crate::SubsetError;
use read_fonts::types::Tag;
use write_fonts::{
    from_obj::ToOwnedTable,
    tables::{gpos::*, jstf::*},
};

// Only the authoritative list participates in instancing and store remapping.
// An original list hidden by a non-null extended offset may be unrelated or
// malformed; retaining its old variation indices after rebuilding is unsafe.
macro_rules! select_header_fields {
    ($table:ident) => {
        if $table.script_list2.is_some() {
            $table.script_list = Default::default();
        }
        if $table.feature_list2.is_some() {
            $table.feature_list = Default::default();
        }
        if $table.lookup_list2.is_some() {
            $table.lookup_list = Default::default();
        }
    };
}

pub(super) fn own_gsub(source: &read_fonts::tables::gsub::Gsub) -> write_fonts::tables::gsub::Gsub {
    let mut table: write_fonts::tables::gsub::Gsub = source.to_owned_table();
    select_header_fields!(table);
    table
}

fn own_lookups<'a, 'b>(
    source: impl Iterator<
        Item = Result<read_fonts::tables::gpos::PositionLookup<'a>, read_fonts::ReadError>,
    >,
    target: impl Iterator<Item = &'b mut PositionLookup>,
    remaining: &mut usize,
) -> Result<(), SubsetError> {
    for (source, target) in source.zip(target) {
        own_position_lookup(
            source.map_err(|_| SubsetError::SubsetTableError(Tag::new(b"GPOS")))?,
            target,
            remaining,
        )?;
    }
    Ok(())
}

// ComputedArray cannot infer the number of zero-byte value records from
// their byte slice. Recover their counts from the borrowed table before
// instancing, preserving the subtable's coverage and lookup matching behavior.
pub(super) fn own_gpos(source: &read_fonts::tables::gpos::Gpos) -> Result<Gpos, SubsetError> {
    let error = || SubsetError::SubsetTableError(Tag::new(b"GPOS"));
    let mut table: Gpos = source.to_owned_table();
    select_header_fields!(table);
    let mut remaining = 200_000;
    let source_lookups = source.lookup_list().map_err(|_| error())?.lookups();
    if let Some(target) = table.lookup_list2.as_mut() {
        own_lookups(
            source_lookups.iter(),
            target.lookups.iter_mut().map(|v| v.as_mut()),
            &mut remaining,
        )?;
    } else if let Some(target) = table.lookup_list.as_mut() {
        own_lookups(
            source_lookups.iter(),
            target.lookups.iter_mut().map(|v| v.as_mut()),
            &mut remaining,
        )?;
    } else {
        return Err(error());
    }
    Ok(table)
}

fn own_position_lookup(
    source: read_fonts::tables::gpos::PositionLookup,
    target: &mut PositionLookup,
    remaining: &mut usize,
) -> Result<(), SubsetError> {
    use read_fonts::tables::gpos as read;
    let error = || SubsetError::SubsetTableError(Tag::new(b"GPOS"));
    match (source, target) {
        (read::PositionLookup::Single(source), PositionLookup::Single(target)) => {
            for (source, target) in source.subtables().iter().zip(&mut target.subtables) {
                own_empty_singles(source.map_err(|_| error())?, target.as_mut(), remaining)?;
            }
        }
        (read::PositionLookup::Pair(source), PositionLookup::Pair(target)) => {
            for (source, target) in source.subtables().iter().zip(&mut target.subtables) {
                own_empty_pairs(source.map_err(|_| error())?, target.as_mut(), remaining)?;
            }
        }
        (read::PositionLookup::Extension(source), PositionLookup::Extension(target)) => {
            for (source, target) in source.subtables().iter().zip(&mut target.subtables) {
                match (source.map_err(|_| error())?, target.as_mut()) {
                    (
                        read::ExtensionSubtable::Single(source),
                        ExtensionSubtable::Single(target),
                    ) => {
                        own_empty_singles(
                            source.extension().map_err(|_| error())?,
                            target.extension.as_mut(),
                            remaining,
                        )?;
                    }
                    (read::ExtensionSubtable::Pair(source), ExtensionSubtable::Pair(target)) => {
                        own_empty_pairs(
                            source.extension().map_err(|_| error())?,
                            target.extension.as_mut(),
                            remaining,
                        )?;
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn own_jstf(source: &read_fonts::tables::jstf::Jstf) -> Result<Jstf, SubsetError> {
    let error = || SubsetError::SubsetTableError(Tag::new(b"JSTF"));
    let mut table: Jstf = source.to_owned_table();
    let mut remaining = 200_000;
    for (record, target) in source
        .jstf_script_records()
        .iter()
        .zip(&mut table.jstf_script_records)
    {
        let source = record
            .jstf_script(source.offset_data())
            .map_err(|_| error())?;
        let target = target.jstf_script.as_mut();
        if let (Some(source), Some(target)) = (
            source
                .def_jstf_lang_sys()
                .transpose()
                .map_err(|_| error())?,
            target.def_jstf_lang_sys.as_mut(),
        ) {
            own_jstf_language(&source, target, &mut remaining)?;
        }
        for (source_lang, target_lang) in source
            .jstf_lang_sys_records()
            .iter()
            .zip(&mut target.jstf_lang_sys_records)
        {
            let source_lang = source_lang
                .jstf_lang_sys(source.offset_data())
                .map_err(|_| error())?;
            own_jstf_language(
                &source_lang,
                target_lang.jstf_lang_sys.as_mut(),
                &mut remaining,
            )?;
        }
    }
    Ok(table)
}

fn own_jstf_language(
    source: &read_fonts::tables::jstf::JstfLangSys,
    target: &mut JstfLangSys,
    remaining: &mut usize,
) -> Result<(), SubsetError> {
    let error = || SubsetError::SubsetTableError(Tag::new(b"JSTF"));
    for (source, target) in source
        .jstf_priorities()
        .iter()
        .zip(&mut target.jstf_priorities)
    {
        let source = source.map_err(|_| error())?;
        let target = target.as_mut();
        for (source, target) in [
            (
                source.shrinkage_jstf_max(),
                target.shrinkage_jstf_max.as_mut(),
            ),
            (
                source.extension_jstf_max(),
                target.extension_jstf_max.as_mut(),
            ),
        ] {
            if let (Some(source), Some(target)) = (source.transpose().map_err(|_| error())?, target)
            {
                for (source, target) in source.lookups().iter().zip(&mut target.lookups) {
                    own_position_lookup(source.map_err(|_| error())?, target.as_mut(), remaining)
                        .map_err(|_| error())?;
                }
            }
        }
    }
    Ok(())
}

fn own_empty_singles(
    source: read_fonts::tables::gpos::SinglePos,
    target: &mut SinglePos,
    remaining: &mut usize,
) -> Result<(), SubsetError> {
    match (source, target) {
        (read_fonts::tables::gpos::SinglePos::Format2(source), SinglePos::Format2(target)) => {
            if source.value_format().is_empty() {
                let count = usize::from(source.value_count());
                *remaining = remaining
                    .checked_sub(count)
                    .ok_or(SubsetError::SubsetTableError(Tag::new(b"GPOS")))?;
                target.value_records = vec![ValueRecord::new(); count];
            }
        }
        (read_fonts::tables::gpos::SinglePos::Format4(source), SinglePos::Format4(target)) => {
            if source.value_format().is_empty() {
                let count = usize::from(source.value_count());
                *remaining = remaining
                    .checked_sub(count)
                    .ok_or(SubsetError::SubsetTableError(Tag::new(b"GPOS")))?;
                target.value_records = vec![ValueRecord::new(); count];
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn own_empty_pairs(
    source: read_fonts::tables::gpos::PairPos,
    target: &mut PairPos,
    remaining: &mut usize,
) -> Result<(), SubsetError> {
    match (source, target) {
        (read_fonts::tables::gpos::PairPos::Format2(source), PairPos::Format2(target)) => {
            if source.value_format1().is_empty() && source.value_format2().is_empty() {
                let rows = source.class1_count() as usize;
                let columns = source.class2_count() as usize;
                target.class1_records = empty_class_records(rows, columns, remaining)?;
            }
        }
        (read_fonts::tables::gpos::PairPos::Format4(source), PairPos::Format4(target)) => {
            if source.value_format1().is_empty() && source.value_format2().is_empty() {
                let rows = source.class1_count() as usize;
                let columns = source.class2_count() as usize;
                target.class1_records = empty_class_records(rows, columns, remaining)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn empty_class_records(
    rows: usize,
    columns: usize,
    remaining: &mut usize,
) -> Result<Vec<Class1Record>, SubsetError> {
    // Empty rows still allocate. With no rows, do not allocate a temporary
    // column vector that would immediately be discarded.
    *remaining = remaining
        .checked_sub(rows.saturating_mul(columns).max(rows))
        .ok_or(SubsetError::SubsetTableError(Tag::new(b"GPOS")))?;
    Ok((0..rows)
        .map(|_| {
            Class1Record::new(vec![
                Class2Record::new(
                    ValueRecord::new(),
                    ValueRecord::new()
                );
                columns
            ])
        })
        .collect())
}
