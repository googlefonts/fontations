use super::*;
use font_test_data::bebuffer::BeBuffer;

fn input() -> Vec<u8> {
    BeBuffer::new()
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
        .push(0u32)
        .push(18u32)
        .push(1u16)
        .push(7u16)
        .to_vec()
}

fn features(defaults: &[&[u16]]) -> Vec<u8> {
    let mut bytes = BeBuffer::new().push(defaults.len() as u16);
    let mut offset = 2 + defaults.len() * 6;
    for default in defaults {
        bytes = bytes.push(Tag::new(b"liga")).push(offset as u16);
        offset += 4 + default.len() * 2;
    }
    for default in defaults {
        bytes = bytes
            .push(0u16)
            .push(default.len() as u16)
            .extend(default.iter().copied());
    }
    bytes.to_vec()
}

#[test]
fn lookup_variations_keep_features_with_no_retained_default_lookups() {
    let input = input();
    let variations = FeatureVariations::read(FontData::new(&input)).unwrap();
    let retained = [7u16].into_iter().collect();
    let alternate = crate::layout::collect_features_with_retained_subs(&variations, &retained);
    assert_eq!(alternate.iter().collect::<Vec<_>>(), [0]);
    let features = features(&[&[], &[1], &[1]]);
    let table = FeatureList::read(FontData::new(&features)).unwrap();
    let pruned = crate::layout::prune_features(
        &table,
        &alternate,
        &retained,
        [0u16, 1, 2].into_iter().collect(),
    );
    assert_eq!(pruned.iter().collect::<Vec<_>>(), [0]);
    assert!(features_with_retained_lookups(&variations, &[1u16].into_iter().collect()).is_empty());
}

#[test]
fn lookup_variations_prevent_default_only_feature_deduplication() {
    let mut input = input();
    let features = features(&[&[1], &[1], &[1]]);
    let table = FeatureList::read(FontData::new(&features)).unwrap();
    let retained = [1u16, 7].into_iter().collect();
    let selected: IntSet<u16> = [0u16, 1, 2].into_iter().collect();
    let mut duplicates =
        crate::layout::find_duplicate_features(&table, &retained, selected.clone());
    assert_eq!(duplicates.get(&1), Some(&0));
    assert_eq!(duplicates.get(&2), Some(&0));
    input[12..14].copy_from_slice(&2u16.to_be_bytes());
    let variations = FeatureVariations::read(FontData::new(&input)).unwrap();
    protect_lookup_variation_features(&variations, &mut duplicates);
    assert_eq!(duplicates.get(&0), Some(&0));
    assert_eq!(duplicates.get(&1), Some(&0));
    assert_eq!(duplicates.get(&2), Some(&2));
    input[12..14].fill(0);
    let variations = FeatureVariations::read(FontData::new(&input)).unwrap();
    let mut duplicates = crate::layout::find_duplicate_features(&table, &retained, selected);
    protect_lookup_variation_features(&variations, &mut duplicates);
    for i in 0..3 {
        assert_eq!(duplicates.get(&i), Some(&i));
    }
}

#[test]
fn lookup_variations_feature_retention_handles_empty_and_invalid_lists() {
    let retained = [7u16].into_iter().collect();
    let mut bytes = input();
    bytes[36..38].fill(0);
    let table = FeatureVariations::read(FontData::new(&bytes)).unwrap();
    assert!(features_with_retained_lookups(&table, &retained).is_empty());
    for range in [14..18, 32..36, 24..28] {
        let mut bytes = input();
        bytes[range].fill(0xff);
        let table = FeatureVariations::read(FontData::new(&bytes)).unwrap();
        assert!(features_with_retained_lookups(&table, &retained).is_empty());
    }
}
