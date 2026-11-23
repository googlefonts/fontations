use super::*;
use font_test_data::bebuffer::BeBuffer;
use read_fonts::types::{F2Dot14, Uint24};

fn variable(value: i16, index: u32) -> Vec<u8> {
    BeBuffer::new().push(2u16).push(value).push(index).to_vec()
}

fn compound(format: u16, children: &[Option<Vec<u8>>]) -> Vec<u8> {
    let mut bytes = BeBuffer::new().push(format).push(children.len() as u8);
    let mut offset = 3 + children.len() * 3;
    for child in children {
        bytes = bytes.push(Uint24::new(if child.is_some() { offset as u32 } else { 0 }));
        offset += child.as_ref().map_or(0, Vec::len);
    }
    let mut bytes = bytes.to_vec();
    for child in children.iter().flatten() {
        bytes.extend(child);
    }
    bytes
}

fn negate(child: Option<Vec<u8>>) -> Vec<u8> {
    let mut bytes = BeBuffer::new()
        .push(5u16)
        .push(Uint24::new(if child.is_some() { 5 } else { 0 }))
        .to_vec();
    if let Some(child) = child {
        bytes.extend(child);
    }
    bytes
}

fn subset<T>(table: &T, plan: &Plan) -> Result<Vec<u8>, SerializeErrorFlags>
where
    for<'a> T: SubsetTable<'a, ArgsForSubset = &'a mut usize, Output = ()>,
{
    let mut s = Serializer::new(1024 * 1024).unwrap();
    s.start_serialize().unwrap();
    table.subset(plan, &mut s, &mut CONDITION_OPERATIONS.clone())?;
    s.end_serialize();
    if s.in_error() {
        return Err(s.error());
    }
    Ok(s.copy_bytes())
}

#[test]
fn conditions_preserve_all_formats_and_remap_variable_values() {
    let axis = BeBuffer::new()
        .push(1u16)
        .push(0u16)
        .push(F2Dot14::from_f32(-0.5))
        .push(F2Dot14::from_f32(0.5))
        .to_vec();
    let mut plan = Plan::default();
    plan.layout_varidx_delta_map.insert(0x10002, (0x70008, 3));
    let bytes = compound(
        3,
        &[
            Some(axis),
            Some(compound(
                4,
                &[
                    Some(variable(-5, 0x10002)),
                    Some(negate(Some(variable(1, NO_VARIATION_INDEX)))),
                ],
            )),
        ],
    );
    let table = Condition::read(FontData::new(&bytes)).unwrap();
    let output = subset(&table, &plan).unwrap();
    let output = Condition::read(FontData::new(&output)).unwrap();
    for coord in [-0.75, 0.0, 0.75] {
        for delta in [0.0, 5.0, 10.0] {
            let coords = [F2Dot14::from_f32(coord)];
            let before = table
                .evaluate(&coords, |index| {
                    assert_eq!(index, 0x10002);
                    Ok::<_, read_fonts::model::Error>(delta)
                })
                .unwrap();
            let after = output
                .evaluate(&coords, |index| {
                    assert_eq!(index, 0x70008);
                    Ok::<_, read_fonts::model::Error>(delta - 3.0)
                })
                .unwrap();
            assert_eq!(before, after);
        }
    }
    let mut indices = IntSet::empty();
    let mut remaining = CONDITION_OPERATIONS;
    collect_condition_indices(
        &output,
        crate::MAX_NESTING_LEVEL,
        &mut remaining,
        &mut indices,
    )
    .unwrap();
    assert_eq!(indices.iter().collect::<Vec<_>>(), [0x70008]);
    let bytes = variable(-5, 0x10002);
    let table = Condition::read(FontData::new(&bytes)).unwrap();
    let output = subset(&table, &plan).unwrap();
    let Condition::Format2VariableValue(output) = Condition::read(FontData::new(&output)).unwrap()
    else {
        panic!()
    };
    assert_eq!(output.default_value(), -2);
    assert_eq!(output.var_index(), 0x70008);
    let output = subset(&table, &Plan::default()).unwrap();
    let Condition::Format2VariableValue(output) = Condition::read(FontData::new(&output)).unwrap()
    else {
        panic!()
    };
    assert_eq!(output.default_value(), -5);
    assert_eq!(output.var_index(), NO_VARIATION_INDEX);
    plan.layout_varidx_delta_map.insert(0x10002, (0, i32::MAX));
    assert_eq!(
        subset(&table, &plan),
        Err(SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW)
    );
}

#[test]
fn conditions_keep_null_boolean_operands_and_condition_set_members() {
    for (bytes, expected) in [
        (
            compound(4, &[Some(variable(-1, NO_VARIATION_INDEX)), None]),
            true,
        ),
        (
            compound(3, &[Some(variable(-1, NO_VARIATION_INDEX)), None]),
            false,
        ),
        (negate(None), false),
        (compound(3, &[]), true),
        (compound(4, &[]), false),
    ] {
        let table = Condition::read(FontData::new(&bytes)).unwrap();
        let output = subset(&table, &Plan::default()).unwrap();
        let output = Condition::read(FontData::new(&output)).unwrap();
        assert_eq!(
            output
                .evaluate(&[], |_| Ok::<_, read_fonts::model::Error>(0.0))
                .unwrap(),
            expected
        );
        if let Condition::Format4Or(table) = output {
            if table.condition_count() != 0 {
                assert_eq!(table.condition_count(), 2);
                assert!(table.condition_offsets()[1].get().is_null());
            }
        }
    }
    let mut bytes = BeBuffer::new()
        .push(3u16)
        .push(0u32)
        .push(14u32)
        .push(22u32)
        .to_vec();
    bytes.extend(variable(-1, NO_VARIATION_INDEX));
    bytes.extend(negate(None));
    let table = ConditionSet::read(FontData::new(&bytes)).unwrap();
    let output = subset(&table, &Plan::default()).unwrap();
    let output = ConditionSet::read(FontData::new(&output)).unwrap();
    assert_eq!(output.condition_count(), 2);
    assert!(matches!(
        output.conditions().get(0).unwrap(),
        Condition::Format2VariableValue(_)
    ));
    assert!(matches!(
        output.conditions().get(1).unwrap(),
        Condition::Format5Negate(_)
    ));
    assert!(!output
        .evaluate(&[], |_| Ok::<_, read_fonts::model::Error>(0.0))
        .unwrap());
}

#[test]
fn condition_serialization_rejects_truncation_and_bounds_tree_work() {
    let plan = Plan::default();
    for bytes in [vec![0, 3, 1], vec![0, 4, 1], vec![0, 5, 255, 255, 255]] {
        let table = Condition::read(FontData::new(&bytes)).unwrap();
        assert_eq!(
            subset(&table, &plan),
            Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)
        );
    }
    let mut bytes = variable(1, NO_VARIATION_INDEX);
    for _ in 1..crate::MAX_NESTING_LEVEL {
        bytes = negate(Some(bytes));
    }
    let table = Condition::read(FontData::new(&bytes)).unwrap();
    assert!(subset(&table, &plan).is_ok());
    bytes = negate(Some(bytes));
    let table = Condition::read(FontData::new(&bytes)).unwrap();
    assert_eq!(
        subset(&table, &plan),
        Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)
    );
    // A small DAG can refer to the same subtree exponentially many times.
    let mut bytes = Vec::new();
    for _ in 0..16 {
        bytes.extend(
            BeBuffer::new()
                .push(3u16)
                .push(2u8)
                .push(Uint24::new(9))
                .push(Uint24::new(9))
                .to_vec(),
        );
    }
    bytes.extend(variable(1, 0x10002));
    let table = Condition::read(FontData::new(&bytes)).unwrap();
    assert_eq!(
        subset(&table, &plan),
        Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)
    );
    let mut indices = IntSet::empty();
    let mut remaining = CONDITION_OPERATIONS;
    assert!(collect_condition_indices(
        &table,
        crate::MAX_NESTING_LEVEL,
        &mut remaining,
        &mut indices
    )
    .is_err());
    assert!(indices.contains(0x10002));
    let bytes = BeBuffer::new().push(1u16).to_vec();
    let table = ConditionSet::read(FontData::new(&bytes)).unwrap();
    assert_eq!(
        subset(&table, &plan),
        Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)
    );
}

#[test]
fn feature_variations_collect_conditional_only_and_earlier_barrier_indices() {
    let mut bytes = BeBuffer::new()
        .push(1u16)
        .push(1u16)
        .push(0u32)
        .push(1u32)
        .push(0u16)
        .push(18u32)
        .push(1u16)
        .push(0u16)
        .push(0u16)
        .push(1u32)
        .push(22u32)
        .push(18u32)
        .push(1u16)
        .push(7u16)
        .to_vec();
    bytes.extend(variable(1, 0x10002));
    let table = FeatureVariations::read(FontData::new(&bytes)).unwrap();
    let features = [(0, 0)].into_iter().collect();
    let lookups = [(7, 0)].into_iter().collect();
    let mut indices = IntSet::empty();
    collect_feature_variation_condition_indices(
        &table,
        &features,
        &lookups,
        &mut indices,
        &mut OpsBudget::new(table.offset_data()),
    );
    assert_eq!(indices.iter().collect::<Vec<_>>(), [0x10002]);
    indices.clear();
    collect_feature_variation_condition_indices(
        &table,
        &features,
        &FastHashMap::default(),
        &mut indices,
        &mut OpsBudget::new(table.offset_data()),
    );
    assert!(indices.is_empty());

    let mut bytes = BeBuffer::new()
        .push(1u16)
        .push(0u16)
        .push(2u32)
        .push(24u32)
        .push(0u32)
        .push(0u32)
        .push(38u32)
        .push(1u16)
        .push(6u32)
        .to_vec();
    bytes.extend(variable(1, 0x10002));
    bytes.extend(
        BeBuffer::new()
            .push(1u16)
            .push(0u16)
            .push(1u16)
            .push(0u16)
            .push(12u32)
            .push(0u16)
            .push(1u16)
            .push(7u16)
            .to_vec(),
    );
    let table = FeatureVariations::read(FontData::new(&bytes)).unwrap();
    collect_feature_variation_condition_indices(
        &table,
        &features,
        &lookups,
        &mut indices,
        &mut OpsBudget::new(table.offset_data()),
    );
    assert_eq!(indices.iter().collect::<Vec<_>>(), [0x10002]);
}

// A FeatureVariations 1.1 table for feature 0: `records` LookupVariationRecords
// share one FeatureLookups whose `conditions` LookupConditionRecords all name
// AND(255 x variable value 0x10001), then one more record names the
// condition variable value 0x20002.
fn shared_lookup_conditions(records: u32, conditions: u32) -> Vec<u8> {
    let shared = 12 + 6 * (records + 1);
    let last = shared + 10 + 8 * conditions;
    let list = last + 18;
    let heavy = list + 4;
    let light = heavy + 3 + 3 * 255 + 8;
    let mut buf = BeBuffer::new().extend([1u16, 1]).extend([0, records + 1]);
    for i in 0..=records {
        buf = buf.push(0u16).push(if i < records { shared } else { last });
    }
    buf = buf.extend([1u16, 0, 0]).push(conditions);
    for _ in 0..conditions {
        buf = buf.extend([heavy - shared, list - shared]);
    }
    buf = buf.extend([1u16, 0, 0]).push(1u32);
    buf = buf.extend([light - last, list - last]).extend([1u16, 0]);
    buf = buf.push(3u16).push(255u8);
    for _ in 0..255 {
        buf = buf.push(Uint24::new(3 + 3 * 255));
    }
    let mut bytes = buf.to_vec();
    bytes.extend(variable(1, 0x10001));
    bytes.extend(variable(1, 0x20002));
    bytes
}

#[test]
fn feature_variations_conditions_share_one_budget() {
    let features = [(0, 0)].into_iter().collect();
    let lookups = [(0, 0)].into_iter().collect();
    let plan = Plan {
        gsub_features_w_duplicates: [(0, 0)].into_iter().collect(),
        gsub_lookups: [(0, 0)].into_iter().collect(),
        ..Default::default()
    };
    // The FeatureVariations table at `bytes`, in a GSUB table of `table_data`.
    let collect = |bytes: &[u8], table_data: FontData| {
        let table = FeatureVariations::read(FontData::new(bytes)).unwrap();
        let mut indices = IntSet::empty();
        let mut budget = OpsBudget::new(table_data);
        collect_feature_variation_condition_indices(
            &table,
            &features,
            &lookups,
            &mut indices,
            &mut budget,
        );
        let mut plan = Plan::default();
        budget.record(Gsub::TAG, &mut plan);
        let exceeded = plan.layout_ops_exceeded.contains(Gsub::TAG);
        (indices.iter().collect::<Vec<_>>(), exceeded)
    };
    let subset = |bytes: &[u8], table_data: FontData| {
        let table = FeatureVariations::read(FontData::new(bytes)).unwrap();
        let mut s = Serializer::new(1024 * 1024).unwrap();
        s.start_serialize().unwrap();
        let mut context = SubsetLayoutContext::new(Gsub::TAG, table_data);
        let ret = table.subset(&plan, &mut s, (&[], &mut context));
        s.end_serialize();
        ret
    };

    // 4 x 4 walks of the 256 operation condition fit the table's budget.
    let bytes = shared_lookup_conditions(4, 4);
    let data = FontData::new(&bytes);
    assert_eq!(collect(&bytes, data), (vec![0x10001, 0x20002], false));
    assert_eq!(subset(&bytes, data), Ok(()));

    // 40 x 40 walks do not: each record no longer gets a budget of its own, so
    // the walks stop before they reach the last record, and the plan records
    // that the table can't be subset.
    let bytes = shared_lookup_conditions(40, 40);
    let data = FontData::new(&bytes);
    assert_eq!(collect(&bytes, data), (vec![0x10001], true));
    assert_eq!(
        subset(&bytes, data),
        Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)
    );

    // The budget is that of the whole GSUB table: in one of 8000 bytes, the
    // same records fit it.
    let gsub = [0; 8000];
    let data = FontData::new(&gsub);
    assert_eq!(collect(&bytes, data), (vec![0x10001, 0x20002], false));
    assert_eq!(subset(&bytes, data), Ok(()));
}
