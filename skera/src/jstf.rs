//! Preserve justification lookups, glyphs, and shared GDEF variation rows.
use crate::{
    offset::SerializeSubset,
    serialize::{SerializeErrorFlags, SerializeResultEmpty, Serializer},
    CollectVariationIndices, FastHashMap, Plan, SubsetError, SubsetState, SubsetTable,
};
use read_fonts::{
    collections::IntSet,
    tables::{jstf::*, layout::Intersect},
    types::{GlyphId, MajorMinor, Offset16, Tag},
    FontRef, MinByteRange, ReadError, TableProvider,
};

const TAG: Tag = Tag::new(b"JSTF");
type Result<T> = std::result::Result<T, SerializeErrorFlags>;
type Args<'a> = (&'a SubsetState, &'a FontRef<'a>);

fn count(remaining: &mut usize, count: usize) -> std::result::Result<(), ReadError> {
    *remaining = remaining
        .checked_sub(count)
        .ok_or(ReadError::MalformedData("too much JSTF data"))?;
    Ok(())
}

fn checked<'a, T: MinByteRange<'a>>(table: &T) -> std::result::Result<(), ReadError> {
    if table.min_table_bytes().len() != table.min_byte_range().len() {
        return Err(ReadError::OutOfBounds);
    }
    Ok(())
}

fn priorities<'a>(
    table: &Jstf<'a>,
    scripts: &IntSet<Tag>,
) -> std::result::Result<Vec<JstfPriority<'a>>, ReadError> {
    checked(table)?;
    if table.version() != MajorMinor::VERSION_1_0 {
        return Err(ReadError::MalformedData("unsupported JSTF version"));
    }
    let mut remaining = 200_000;
    let mut result = Vec::new();
    count(&mut remaining, table.jstf_script_records().len())?;
    for record in table.jstf_script_records() {
        if !scripts.contains(record.jstf_script_tag()) {
            continue;
        }
        let script = record.jstf_script(table.offset_data())?;
        checked(&script)?;
        if let Some(extenders) = script.extender_glyph().transpose()? {
            checked(&extenders)?;
            count(&mut remaining, extenders.glyph_count() as usize)?;
        }
        count(&mut remaining, script.jstf_lang_sys_records().len() + 1)?;
        let mut languages = Vec::new();
        if let Some(language) = script.def_jstf_lang_sys().transpose()? {
            languages.push(language);
        }
        for record in script.jstf_lang_sys_records() {
            languages.push(record.jstf_lang_sys(script.offset_data())?);
        }
        for language in languages {
            checked(&language)?;
            count(&mut remaining, language.jstf_priority_count() as usize)?;
            for priority in language.jstf_priorities().iter() {
                let priority = priority?;
                for list in mod_lists(&priority) {
                    if let Some(list) = list.transpose()? {
                        checked(&list)?;
                        count(&mut remaining, list.lookup_count() as usize)?;
                    }
                }
                for maximum in [priority.shrinkage_jstf_max(), priority.extension_jstf_max()] {
                    if let Some(maximum) = maximum.transpose()? {
                        checked(&maximum)?;
                        count(&mut remaining, maximum.lookup_count() as usize)?;
                        for lookup in maximum.lookups().iter() {
                            let lookup = lookup?;
                            // Contextual positioning is forbidden in JstfMax.
                            use read_fonts::tables::gpos::PositionSubtables;
                            let subtables = match lookup.subtables()? {
                                PositionSubtables::Single(v) => v.len(),
                                PositionSubtables::Pair(v) => v.len(),
                                PositionSubtables::Cursive(v) => v.len(),
                                PositionSubtables::MarkToBase(v) => v.len(),
                                PositionSubtables::MarkToLig(v) => v.len(),
                                PositionSubtables::MarkToMark(v) => v.len(),
                                PositionSubtables::Contextual(_)
                                | PositionSubtables::ChainContextual(_) => {
                                    return Err(ReadError::MalformedData("contextual JSTF lookup"));
                                }
                                PositionSubtables::EmptyExtension => 0,
                            };
                            count(&mut remaining, subtables)?;
                        }
                    }
                }
                result.push(priority);
            }
        }
    }
    Ok(result)
}

fn mod_lists<'a>(
    priority: &JstfPriority<'a>,
) -> [Option<std::result::Result<JstfModList<'a>, ReadError>>; 8] {
    [
        priority.gsub_shrinkage_enable(),
        priority.gsub_shrinkage_disable(),
        priority.gpos_shrinkage_enable(),
        priority.gpos_shrinkage_disable(),
        priority.gsub_extension_enable(),
        priority.gsub_extension_disable(),
        priority.gpos_extension_enable(),
        priority.gpos_extension_disable(),
    ]
}

pub(crate) fn validate(table: &Jstf) -> std::result::Result<(), ReadError> {
    priorities(table, &IntSet::all()).map(|_| ())
}

pub(crate) fn closure(font: &FontRef, plan: &mut Plan) -> std::result::Result<(), ReadError> {
    let table = font.jstf()?;
    for priority in priorities(&table, &plan.layout_scripts)? {
        for (index, list) in [
            priority.gsub_shrinkage_enable(),
            priority.gsub_extension_enable(),
            priority.gpos_shrinkage_enable(),
            priority.gpos_extension_enable(),
        ]
        .into_iter()
        .enumerate()
        {
            if let Some(list) = list.transpose()? {
                let target = if index < 2 {
                    &mut plan.jstf_gsub_lookups
                } else {
                    &mut plan.jstf_gpos_lookups
                };
                target.extend(list.lookup_indices().iter().map(|index| index.get()));
            }
        }
    }
    for record in table.jstf_script_records() {
        if !plan.layout_scripts.contains(record.jstf_script_tag()) {
            continue;
        }
        let script = record.jstf_script(table.offset_data())?;
        if let Some(extenders) = script.extender_glyph().transpose()? {
            plan.glyphset_gsub.extend(
                extenders
                    .extender_glyphs()
                    .iter()
                    .map(|glyph| GlyphId::from(glyph.get())),
            );
        }
    }
    Ok(())
}

pub(crate) fn collect_variation_indices(font: &FontRef, plan: &Plan, indices: &mut IntSet<u32>) {
    let Ok(table) = font.jstf() else {
        return;
    };
    let Ok(priorities) = priorities(&table, &plan.layout_scripts) else {
        return;
    };
    for priority in priorities {
        for maximum in [priority.shrinkage_jstf_max(), priority.extension_jstf_max()] {
            if let Some(Ok(maximum)) = maximum {
                for lookup in maximum.lookups().iter().flatten() {
                    if let Ok(subtables) = lookup.subtables() {
                        subtables.collect_variation_indices(plan, indices);
                    }
                }
            }
        }
    }
}

pub(crate) fn subset(
    font: &FontRef,
    plan: &Plan,
    state: &SubsetState,
    s: &mut Serializer,
) -> std::result::Result<(), SubsetError> {
    let error = || SubsetError::SubsetTableError(TAG);
    let table = font.jstf().map_err(|_| error())?;
    validate(&table).map_err(|_| error())?;
    s.embed(table.version()).map_err(|_| error())?;
    let count_pos = s.embed(0_u16).map_err(|_| error())?;
    let mut count = 0;
    for record in table.jstf_script_records() {
        if !plan.layout_scripts.contains(record.jstf_script_tag()) {
            continue;
        }
        s.embed(record.jstf_script_tag()).map_err(|_| error())?;
        let pos = s.embed(0_u16).map_err(|_| error())?;
        let script = record
            .jstf_script(table.offset_data())
            .map_err(|_| error())?;
        Offset16::serialize_subset(&script, s, plan, (state, font), pos).map_err(|_| error())?;
        count += 1;
    }
    s.copy_assign(count_pos, count as u16);
    Ok(())
}

fn optional<'a, T: SubsetTable<'a, Output = ()>>(
    table: Option<std::result::Result<T, ReadError>>,
    plan: &Plan,
    s: &mut Serializer,
    args: T::ArgsForSubset,
    pos: usize,
) -> Result<()> {
    if let Some(table) = table
        .transpose()
        .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
    {
        Offset16::serialize_subset(&table, s, plan, args, pos).is_empty()?;
    }
    Ok(())
}

impl<'a> SubsetTable<'a> for JstfScript<'a> {
    type ArgsForSubset = Args<'a>;
    type Output = ();
    fn subset(&self, plan: &Plan, s: &mut Serializer, args: Args<'a>) -> Result<()> {
        let extender = s.embed(0_u16)?;
        let default = s.embed(0_u16)?;
        s.embed(self.jstf_lang_sys_count())?;
        optional(self.extender_glyph(), plan, s, (), extender)?;
        optional(self.def_jstf_lang_sys(), plan, s, args, default)?;
        for record in self.jstf_lang_sys_records() {
            s.embed(record.jstf_lang_sys_tag())?;
            let pos = s.embed(0_u16)?;
            let language = record
                .jstf_lang_sys(self.offset_data())
                .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;
            Offset16::serialize_subset(&language, s, plan, args, pos)?;
        }
        Ok(())
    }
}

impl SubsetTable<'_> for ExtenderGlyph<'_> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(&self, plan: &Plan, s: &mut Serializer, _: ()) -> Result<()> {
        let mut glyphs = self
            .extender_glyphs()
            .iter()
            .filter_map(|glyph| plan.glyph_map.get(&GlyphId::from(glyph.get())).copied())
            .collect::<Vec<_>>();
        glyphs.sort_unstable();
        glyphs.dedup();
        s.embed(glyphs.len() as u16)?;
        for glyph in glyphs {
            s.embed(glyph.to_u32() as u16)?;
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for JstfLangSys<'a> {
    type ArgsForSubset = Args<'a>;
    type Output = ();
    fn subset(&self, plan: &Plan, s: &mut Serializer, args: Args<'a>) -> Result<()> {
        s.embed(self.jstf_priority_count())?;
        for priority in self.jstf_priorities().iter() {
            let priority =
                priority.map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;
            let pos = s.embed(0_u16)?;
            Offset16::serialize_subset(&priority, s, plan, args, pos)?;
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for JstfPriority<'a> {
    type ArgsForSubset = Args<'a>;
    type Output = ();
    fn subset(&self, plan: &Plan, s: &mut Serializer, args: Args<'a>) -> Result<()> {
        let mut offsets = [0; 10];
        for pos in &mut offsets {
            *pos = s.embed(0_u16)?;
        }
        for (index, list) in mod_lists(self).into_iter().enumerate() {
            let pos = offsets[if index < 4 { index } else { index + 1 }];
            let mapping = if index % 4 < 2 {
                &plan.gsub_lookups
            } else {
                &plan.gpos_lookups
            };
            optional(list, plan, s, mapping, pos)?;
        }
        for (maximum, pos) in [
            (self.shrinkage_jstf_max(), offsets[4]),
            (self.extension_jstf_max(), offsets[9]),
        ] {
            if let Some(maximum) = maximum
                .transpose()
                .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
            {
                s.push()?;
                let count_pos = s.embed(0_u16)?;
                let mut count = 0_u16;
                for lookup in maximum.lookups().iter() {
                    let lookup = lookup
                        .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;
                    if !lookup
                        .intersects(&plan.glyphset_gsub)
                        .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
                    {
                        continue;
                    }
                    let offset = s.embed(0_u16)?;
                    Offset16::serialize_subset(
                        &lookup,
                        s,
                        plan,
                        (args.0, args.1, &plan.gpos_lookups),
                        offset,
                    )?;
                    count += 1;
                }
                s.copy_assign(count_pos, count);
                let child = s
                    .pop_pack(true)
                    .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_OTHER)?;
                s.add_link(
                    pos..pos + 2,
                    child,
                    crate::serialize::OffsetWhence::Head,
                    0,
                    false,
                )?;
            }
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for JstfModList<'_> {
    type ArgsForSubset = &'a FastHashMap<u16, u16>;
    type Output = ();
    fn subset(&self, _: &Plan, s: &mut Serializer, mapping: Self::ArgsForSubset) -> Result<()> {
        let mut lookups = self
            .lookup_indices()
            .iter()
            .filter_map(|index| mapping.get(&index.get()).copied())
            .collect::<Vec<_>>();
        lookups.sort_unstable();
        lookups.dedup();
        if lookups.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        s.embed(lookups.len() as u16)?;
        for index in lookups {
            s.embed(index)?;
        }
        Ok(())
    }
}
