use crate::{variations::solver::Triple, Plan, SubsetError};
use fnv::FnvHashMap;
use hashbrown::{HashMap, HashSet};
use skrifa::{
    raw::{
        tables::stat::{AxisValue, AxisValueTableFlags, Stat},
        types::NameId,
        ReadError, TableProvider, TopLevelTable,
    },
    string::LocalizedStrings,
    FontRef, MetadataProvider, Tag,
};
use write_fonts::{
    from_obj::ToOwnedTable,
    tables::{
        head::{Head, MacStyle},
        name::{Name, NameRecord},
        os2::{Os2, SelectionFlags},
    },
    FontBuilder,
};

fn axis_values_from_axis_limits<'a>(
    stat: &'a Stat,
    axis_limits: &FnvHashMap<Tag, Triple<f64>>,
) -> Result<Vec<AxisValue<'a>>, SubsetError> {
    let Some(Ok(subtable)) = stat.offset_to_axis_values() else {
        unreachable!("stat table has no axis values subtable, even though we checked it did");
    };
    let axis_tags = stat
        .design_axes()
        .map_err(SubsetError::ReadError)?
        .iter()
        .map(|axis| axis.axis_tag())
        .collect::<Vec<_>>();
    Ok(subtable
        .axis_values()
        .iter()
        .flatten()
        .filter(|v| match v {
            AxisValue::Format1(v) => {
                let value = v.value().to_f64();
                if let Some(tag) = axis_tags.get(v.axis_index() as usize) {
                    if let Some(limit) = axis_limits.get(tag) {
                        return value >= limit.minimum && value <= limit.maximum;
                    }
                }
                false
            }
            AxisValue::Format2(v) => {
                let value = v.nominal_value().to_f64();
                if let Some(tag) = axis_tags.get(v.axis_index() as usize) {
                    if let Some(limit) = axis_limits.get(tag) {
                        return value >= limit.minimum && value <= limit.maximum;
                    }
                }
                false
            }
            AxisValue::Format3(v) => {
                let value = v.value().to_f64();
                if let Some(tag) = axis_tags.get(v.axis_index() as usize) {
                    if let Some(limit) = axis_limits.get(tag) {
                        return value >= limit.minimum && value <= limit.maximum;
                    }
                }
                false
            }
            AxisValue::Format4(v) => {
                for axis_value in v.axis_values() {
                    let value = axis_value.value().to_f64();
                    if let Some(tag) = axis_tags.get(axis_value.axis_index() as usize) {
                        if let Some(limit) = axis_limits.get(tag) {
                            if value < limit.minimum || value > limit.maximum {
                                return false;
                            }
                        }
                    }
                }
                true
            }
        })
        .collect())
}

fn sort_axis_value_tables<'a>(axis_value_tables: Vec<AxisValue<'a>>) -> Vec<AxisValue<'a>> {
    let (mut format_4, non_format_4): (Vec<_>, Vec<_>) = axis_value_tables
        .into_iter()
        .partition(|v| matches!(v, AxisValue::Format4(_)));
    format_4.sort_by_key(|v| match v {
        AxisValue::Format4(v) => v.axis_values().len(),
        _ => unreachable!(),
    });
    let mut seen_axis = HashSet::new();
    let mut results = vec![];
    for value in format_4.into_iter() {
        let axes_indices: HashSet<u16> = HashSet::from_iter(match &value {
            AxisValue::Format4(v) => v
                .axis_values()
                .iter()
                .map(|av| av.axis_index())
                .collect::<Vec<_>>(),
            _ => unreachable!(),
        });
        let min_index = axes_indices.iter().copied().min().unwrap();
        if !seen_axis.contains(&min_index) {
            seen_axis.extend(axes_indices);
            results.push((min_index, value));
        }
    }
    for value in non_format_4.into_iter() {
        let axis_index = match &value {
            AxisValue::Format1(v) => v.axis_index(),
            AxisValue::Format2(v) => v.axis_index(),
            AxisValue::Format3(v) => v.axis_index(),
            _ => unreachable!(),
        };
        if !seen_axis.contains(&axis_index) {
            seen_axis.insert(axis_index);
            results.push((axis_index, value));
        }
    }
    results.sort_by_key(|(axis_index, _)| *axis_index);
    results.into_iter().map(|(_, v)| v).collect()
}

pub fn update_name_table_from_stat(plan: &Plan, fontref: &FontRef) -> Result<Vec<u8>, SubsetError> {
    let stat = fontref.stat().map_err(SubsetError::ReadError)?;
    let mut axis_value_tables = axis_values_from_axis_limits(&stat, &plan.user_axes_location)?;
    // Check they exist here
    axis_value_tables.retain(|v| {
        !v.flags()
            .contains(AxisValueTableFlags::ELIDABLE_AXIS_VALUE_NAME)
    });
    let axis_value_tables = sort_axis_value_tables(axis_value_tables);
    //     update_name_records(fontref, &axis_value_tables)
    // }

    // fn update_name_records<'a>(
    //     fontref: FontRef<'a>,
    //     axis_value_tables: Vec<AxisValue<'a>>,
    // ) -> Result<FontRef<'a>, GftoolsError> {
    let mut name_table: Name = fontref
        .name()
        .map_err(SubsetError::ReadError)?
        .to_owned_table();
    let fallback_version = fontref
        .head()
        .map_err(SubsetError::ReadError)?
        .font_revision()
        .to_string();
    let vendor_id = fontref
        .os2()
        .map_err(SubsetError::ReadError)?
        .ach_vend_id()
        .to_string();
    let elided_fallback_name_id = stat.elided_fallback_name_id();
    let axis_value_name_ids: Vec<NameId> = axis_value_tables
        .iter()
        .map(|v| v.value_name_id())
        .collect();
    let (ribbi_name_ids, non_ribbi_name_ids): (Vec<_>, Vec<_>) = axis_value_name_ids
        .iter()
        .copied()
        .partition(|id| is_ribbi(fontref.localized_strings(*id)));
    let elided_name_is_ribbi = elided_fallback_name_id
        .map(|id| is_ribbi(fontref.localized_strings(id)))
        .unwrap_or(false);
    let mut name_records = name_table.name_record;
    let platforms: HashSet<_> = name_records
        .iter()
        .map(|record| (record.platform_id, record.encoding_id, record.language_id))
        .collect();
    for platform in platforms.into_iter() {
        let records_for_platform = name_records
            .iter()
            .filter(|record| {
                record.platform_id == platform.0
                    && record.encoding_id == platform.1
                    && record.language_id == platform.2
            })
            .collect::<Vec<_>>();
        if !records_for_platform
            .iter()
            .any(|record| record.name_id == NameId::FAMILY_NAME)
            || !records_for_platform
                .iter()
                .any(|record| record.name_id == NameId::SUBFAMILY_NAME)
            || (elided_fallback_name_id.is_some()
                && !records_for_platform
                    .iter()
                    .any(|record| record.name_id == elided_fallback_name_id.unwrap()))
        {
            continue;
        }
        let mut subfamily_name = ribbi_name_ids
            .iter()
            .map(|id| {
                records_for_platform
                    .iter()
                    .find(|record| record.name_id == *id)
                    .map(|record| record.string.to_string())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" ");
        let mut typo_subfamily_name = if non_ribbi_name_ids.is_empty() {
            None
        } else {
            axis_value_name_ids
                .iter()
                .map(|id| {
                    records_for_platform
                        .iter()
                        .find(|record| record.name_id == *id)
                        .map(|record| record.string.to_string())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join(" ")
                .into()
        };
        // If neither subFamilyName and typographic SubFamilyName exist
        // we will use the STAT's elidedFallbackName
        if subfamily_name.is_empty() && typo_subfamily_name.is_none() {
            if elided_name_is_ribbi {
                subfamily_name = records_for_platform
                    .iter()
                    .find(|record| record.name_id == elided_fallback_name_id.unwrap())
                    .map(|record| record.string.to_string())
                    .unwrap_or_default();
            } else {
                typo_subfamily_name = records_for_platform
                    .iter()
                    .find(|record| Some(record.name_id) == elided_fallback_name_id)
                    .map(|record| record.string.to_string())
                    .unwrap_or_default()
                    .into();
            }
        }

        let family_name_suffix = non_ribbi_name_ids
            .iter()
            .filter_map(|id| {
                records_for_platform
                    .iter()
                    .find(|record| record.name_id == *id)
                    .map(|record| record.string.to_string())
                    .filter(|s| !s.is_empty())
            })
            .collect::<Vec<_>>()
            .join(" ");
        update_name_table_style_records(
            &mut name_records,
            family_name_suffix,
            subfamily_name,
            typo_subfamily_name,
            platform,
            &fallback_version,
            vendor_id.clone(),
        )
        .map_err(SubsetError::ReadError)?;
    }

    if fontref.fvar().is_err() {
        name_records.retain(|record| record.name_id != NameId::VARIATIONS_POSTSCRIPT_NAME_PREFIX);
    }

    // Sort the name records, rebuild the name table
    name_table.name_record = name_records;
    name_table.name_record.sort();
    let mut new_font = FontBuilder::new();
    new_font
        .add_table(&name_table)
        .map_err(|_| SubsetError::SubsetTableError(Stat::TAG))?;
    new_font.copy_missing_tables(fontref.clone());
    Ok(new_font.build())
}

fn is_ribbi(strings: LocalizedStrings) -> bool {
    strings
        .map(|s| s.to_string())
        .any(|s| s.contains("Regular") || s.contains("Italic") || s.contains("Bold"))
}

fn update_name_table_style_records(
    name_records: &mut Vec<NameRecord>,
    family_name_suffix: String,
    subfamily_name: String,
    typo_subfamily_name: Option<String>,
    platform: (u16, u16, u16),
    fallback_version: &str,
    vendor_id: String,
) -> Result<(), ReadError> {
    let relevant_records = name_records
        .iter()
        .filter(|&record| {
            record.platform_id == platform.0
                && record.encoding_id == platform.1
                && record.language_id == platform.2
        })
        .cloned()
        .collect::<Vec<_>>();
    let Some(current_family_name) = relevant_records
        .iter()
        .find(|record| record.name_id == NameId::TYPOGRAPHIC_FAMILY_NAME)
        .or_else(|| {
            relevant_records
                .iter()
                .find(|record| record.name_id == NameId::FAMILY_NAME)
        })
        .map(|record| record.string.to_string())
    else {
        return Ok(());
    };
    let mut new_records = HashMap::new();
    new_records.insert(NameId::FAMILY_NAME, current_family_name.clone());
    new_records.insert(
        NameId::SUBFAMILY_NAME,
        if subfamily_name.is_empty() {
            "Regular".to_string()
        } else {
            subfamily_name
        },
    );
    if let Some(tsfn) = typo_subfamily_name {
        new_records.insert(
            NameId::FAMILY_NAME,
            format!("{} {}", current_family_name.clone(), family_name_suffix),
        );
        new_records.insert(NameId::TYPOGRAPHIC_FAMILY_NAME, current_family_name);
        new_records.insert(NameId::TYPOGRAPHIC_SUBFAMILY_NAME, tsfn);
    } else {
        name_records.retain(|record| {
            !(record.platform_id == platform.0
                && record.encoding_id == platform.1
                && record.language_id == platform.2
                && (record.name_id == NameId::TYPOGRAPHIC_FAMILY_NAME
                    || record.name_id == NameId::TYPOGRAPHIC_SUBFAMILY_NAME))
        });
    }

    let new_family_name = new_records
        .get(&NameId::TYPOGRAPHIC_FAMILY_NAME)
        .cloned()
        .unwrap_or_else(|| new_records.get(&NameId::FAMILY_NAME).cloned().unwrap());
    let new_style_name = new_records
        .get(&NameId::TYPOGRAPHIC_SUBFAMILY_NAME)
        .cloned()
        .unwrap_or_else(|| new_records.get(&NameId::SUBFAMILY_NAME).cloned().unwrap());
    new_records.insert(
        NameId::FULL_NAME,
        format!("{} {}", new_family_name, new_style_name),
    );
    let ps_prefix = relevant_records
        .iter()
        .find(|record| record.name_id == NameId::VARIATIONS_POSTSCRIPT_NAME_PREFIX)
        .map(|record| record.string.to_string());
    new_records.insert(
        NameId::POSTSCRIPT_NAME,
        _update_ps_record(
            ps_prefix.as_ref().unwrap_or(&new_family_name),
            &new_style_name,
        ),
    );
    let font_version = if let Some(id) = name_records
        .iter()
        .find(|record| {
            record.name_id == NameId::VERSION_STRING
                && record.platform_id == 3
                && record.encoding_id == 1
                && record.language_id == 0x409
        })
        .map(|record| record.string.to_string())
    {
        id
    } else {
        fallback_version.to_string()
    };
    if let Some(current_unique_id) = relevant_records
        .iter()
        .find(|record| record.name_id == NameId::UNIQUE_ID)
        .map(|record| record.string.to_string())
    {
        let current_full_font_name = relevant_records
            .iter()
            .find(|record| record.name_id == NameId::FULL_NAME)
            .map(|record| record.string.to_string());
        let current_postscript_name = relevant_records
            .iter()
            .find(|record| record.name_id == NameId::POSTSCRIPT_NAME)
            .map(|record| record.string.to_string());
        let unique_id = _update_unique_id_name_record(
            &current_unique_id,
            current_full_font_name.as_ref(),
            current_postscript_name.as_ref(),
            &new_records,
            font_version,
            vendor_id,
        );
        new_records.insert(NameId::UNIQUE_ID, unique_id);
    }
    // Now either adjust or add records into name_records
    for (name_id, new_string) in new_records.into_iter() {
        if let Some(record) = name_records.iter_mut().find(|record| {
            record.name_id == name_id
                && record.platform_id == platform.0
                && record.encoding_id == platform.1
                && record.language_id == platform.2
        }) {
            record.string = new_string.into();
        } else {
            name_records.push(NameRecord {
                platform_id: platform.0,
                encoding_id: platform.1,
                language_id: platform.2,
                name_id,
                string: new_string.into(),
            });
        }
    }

    Ok(())
}

fn _update_unique_id_name_record(
    current_record: &str,
    current_full_font_name: Option<&String>,
    current_postscript_name: Option<&String>,
    new_records: &HashMap<NameId, String>,
    font_version: String,
    vendor_id: String,
) -> String {
    if let Some(current_full) = current_full_font_name {
        if current_record.contains(current_full) {
            let new_full = new_records
                .get(&NameId::FULL_NAME)
                .unwrap_or(current_full)
                .to_string();
            return current_record.replace(current_full, &new_full);
        }
    }
    if let Some(current_ps) = current_postscript_name {
        if current_record.contains(current_ps) {
            let new_ps = new_records
                .get(&NameId::POSTSCRIPT_NAME)
                .unwrap_or(current_ps)
                .to_string();
            return current_record.replace(current_ps, &new_ps);
        }
    }
    // Create a new string
    let psname = new_records
        .get(&NameId::POSTSCRIPT_NAME)
        .map(|x| x.as_str())
        .unwrap_or(
            current_postscript_name
                .map(|s| s.as_str())
                .unwrap_or("Unknown"),
        );
    let vendor = vendor_id
        .chars()
        .filter(|c| c.is_ascii())
        .collect::<String>()
        .trim()
        .to_string();

    format!("{};{};{}", font_version, vendor, psname)
}

fn _update_ps_record(family_name: &str, style_name: &str) -> String {
    let mut ps_name = format!("{}-{}", family_name, style_name);
    ps_name.retain(|c| c.is_ascii_alphanumeric() || c == '-');
    if ps_name.len() > 127 {
        format!("{}...", &ps_name[..124])
    } else {
        ps_name
    }
}

pub(crate) fn set_ribbi_bits(font_data: Vec<u8>) -> Result<Vec<u8>, SubsetError> {
    let font = FontRef::new(&font_data).map_err(SubsetError::ReadError)?;
    let Some(english_ribbi_style) = font
        .localized_strings(NameId::SUBFAMILY_NAME)
        .english_or_first()
    else {
        return Ok(font_data);
    };
    let style_map_style_name = english_ribbi_style.to_string().to_lowercase();

    let mut head: Head = font
        .head()
        .map_err(SubsetError::ReadError)?
        .to_owned_table();
    let mut os2: Os2 = font.os2().map_err(SubsetError::ReadError)?.to_owned_table();
    os2.fs_selection
        .remove(SelectionFlags::ITALIC | SelectionFlags::BOLD | SelectionFlags::REGULAR);
    match style_map_style_name.as_str() {
        "bold" => {
            head.mac_style = MacStyle::BOLD;
            os2.fs_selection.insert(SelectionFlags::BOLD);
        }
        "bold italic" => {
            head.mac_style = MacStyle::BOLD | MacStyle::ITALIC;
            os2.fs_selection.insert(SelectionFlags::BOLD);
            os2.fs_selection.insert(SelectionFlags::ITALIC);
        }
        "italic" => {
            head.mac_style = MacStyle::ITALIC;
            os2.fs_selection.insert(SelectionFlags::ITALIC);
        }
        _ => {}
    }

    let mut new_font = FontBuilder::new();
    new_font
        .add_table(&head)
        .map_err(|_| SubsetError::SubsetTableError(Head::TAG))?;
    new_font
        .add_table(&os2)
        .map_err(|_| SubsetError::SubsetTableError(Os2::TAG))?;
    new_font.copy_missing_tables(font);
    Ok(new_font.build())
}
