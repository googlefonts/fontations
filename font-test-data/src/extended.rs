//! Synthetic ISO Open Font Format extended-table fixtures.

/// Builds an SFNT from raw tables without calculating checksums.
pub fn font(tables: &[([u8; 4], &[u8])]) -> Vec<u8> {
    let mut tables = tables.to_vec();
    tables.sort_by_key(|(tag, _)| *tag);
    let mut data = vec![0; 12 + 16 * tables.len()];
    data[..4].copy_from_slice(&0x00010000u32.to_be_bytes());
    data[4..6].copy_from_slice(&(tables.len() as u16).to_be_bytes());
    for (i, (tag, table)) in tables.into_iter().enumerate() {
        let record = 12 + i * 16;
        let offset = data.len() as u32;
        data[record..record + 4].copy_from_slice(&tag);
        data[record + 8..record + 12].copy_from_slice(&offset.to_be_bytes());
        data[record + 12..record + 16].copy_from_slice(&(table.len() as u32).to_be_bytes());
        data.extend_from_slice(table);
        data.resize((data.len() + 3) & !3, 0);
    }
    data
}

/// Builds a version 0.5 maximum profile with a 16- or 24-bit count.
pub fn maxp(count: u32, extended: bool) -> Vec<u8> {
    let mut data = 0x00005000u32.to_be_bytes().to_vec();
    if extended {
        data.extend_from_slice(&count.to_be_bytes()[1..]);
    } else {
        data.extend_from_slice(&(count as u16).to_be_bytes());
    }
    data
}

/// Builds a metric header with a 16- or 32-bit long-metric count.
pub fn metric_header(count: u32, extended: bool, vertical: bool) -> Vec<u8> {
    let version = if vertical {
        0x00011000u32
    } else {
        0x00010000u32
    };
    let mut data = version.to_be_bytes().to_vec();
    for value in [
        800i16, -200, 50, 2000, -100, -100, 2000, 1, 0, 0, 0, 0, 0, 0, 0,
    ] {
        data.extend_from_slice(&value.to_be_bytes());
    }
    if extended {
        data.extend_from_slice(&count.to_be_bytes());
    } else {
        data.extend_from_slice(&(count as u16).to_be_bytes());
    }
    data
}

/// Builds long metric records followed by trailing side bearings.
pub fn metric_data(count: u32, trailing: u32, advance: u16) -> Vec<u8> {
    let mut data = Vec::new();
    for i in 0..count {
        data.extend_from_slice(&(advance + (i % 100) as u16).to_be_bytes());
        data.extend_from_slice(&(-((i % 300) as i16)).to_be_bytes());
    }
    for i in 0..trailing {
        data.extend_from_slice(&(-(10 + i as i16)).to_be_bytes());
    }
    data
}

/// Builds independent legacy/extended metrics, optionally with an outline tag.
/// Extended metrics cover 70,002 glyphs; legacy metrics cover three.
pub fn metrics_font(extended: bool, legacy: bool, outline_tag: Option<[u8; 4]>) -> Vec<u8> {
    let mut tables = Vec::new();
    let mut head = vec![0; 54];
    head[..4].copy_from_slice(&0x00010000u32.to_be_bytes());
    head[18..20].copy_from_slice(&1000u16.to_be_bytes());
    tables.push((*b"head", head));
    if extended {
        tables.extend([
            (*b"MAXP", maxp(70002, true)),
            (*b"HHEA", metric_header(70000, true, false)),
            (*b"HMTX", metric_data(70000, 2, 1000)),
            (*b"VHEA", metric_header(70000, true, true)),
            (*b"VMTX", metric_data(70000, 2, 1200)),
        ]);
    }
    if legacy {
        tables.extend([
            (*b"maxp", maxp(3, false)),
            (*b"hhea", metric_header(2, false, false)),
            (*b"hmtx", metric_data(2, 1, 500)),
            (*b"vhea", metric_header(2, false, true)),
            (*b"vmtx", metric_data(2, 1, 600)),
        ]);
    }
    if let Some(tag) = outline_tag {
        tables.push((tag, Vec::new()));
    }
    let tables: Vec<_> = tables
        .iter()
        .map(|(tag, bytes)| (*tag, bytes.as_slice()))
        .collect();
    font(&tables)
}
