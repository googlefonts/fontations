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

/// Builds a one-contour glyph using uncompressed coordinates and curve flags.
pub fn simple_glyph(points: &[(i16, i16, u8)]) -> Vec<u8> {
    let mut data = Vec::new();
    for value in [
        1,
        points.iter().map(|p| p.0).min().unwrap(),
        points.iter().map(|p| p.1).min().unwrap(),
        points.iter().map(|p| p.0).max().unwrap(),
        points.iter().map(|p| p.1).max().unwrap(),
    ] {
        data.extend_from_slice(&value.to_be_bytes());
    }
    data.extend_from_slice(&((points.len() - 1) as u16).to_be_bytes());
    data.extend_from_slice(&0u16.to_be_bytes());
    data.extend(points.iter().map(|p| p.2));
    for axis in [0, 1] {
        let mut previous = 0i16;
        for &(x, y, _) in points {
            let value = if axis == 0 { x } else { y };
            data.extend_from_slice(&value.wrapping_sub(previous).to_be_bytes());
            previous = value;
        }
    }
    data.resize((data.len() + 1) & !1, 0);
    data
}

/// Builds uppercase/legacy outlines with independently selected metrics.
/// GLYF glyph 1 references cubic glyph 65,536 using a 24-bit component ID.
pub fn outlines_font(
    extended: bool,
    legacy: bool,
    extended_metrics: bool,
    long_loca: bool,
) -> Vec<u8> {
    fn profile(count: u32, extended: bool) -> Vec<u8> {
        let mut data = maxp(count, extended);
        data[..4].copy_from_slice(&0x00010000u32.to_be_bytes());
        data.extend_from_slice(&[0; 26]);
        data
    }
    fn locations(offsets: &[u32], long: bool) -> Vec<u8> {
        let mut data = Vec::new();
        for offset in offsets {
            if long {
                data.extend_from_slice(&offset.to_be_bytes());
            } else {
                data.extend_from_slice(&((offset / 2) as u16).to_be_bytes());
            }
        }
        data
    }
    let count = if extended { 65537 } else { 3 };
    let mut head = vec![0; 54];
    head[..4].copy_from_slice(&0x00010000u32.to_be_bytes());
    head[18..20].copy_from_slice(&1000u16.to_be_bytes());
    head[50..52].copy_from_slice(&(long_loca as i16).to_be_bytes());
    let mut tables = vec![(*b"head", head)];
    if extended {
        let mut glyf = Vec::new();
        for value in [-1i16, 0, 0, 80, 80] {
            glyf.extend_from_slice(&value.to_be_bytes());
        }
        glyf.extend_from_slice(&0x2002u16.to_be_bytes());
        glyf.extend_from_slice(&[1, 0, 0]); // Glyph 65,536.
        glyf.extend_from_slice(&[0, 0, 0]); // Byte offsets and padding.
        let mut offsets = vec![glyf.len() as u32; count + 1];
        offsets[0] = 0;
        offsets[1] = 0;
        glyf.extend(simple_glyph(&[
            (0, 0, 0x80),
            (80, 0, 0x80),
            (80, 80, 0x80),
            (0, 80, 0x80),
        ]));
        offsets[count] = glyf.len() as u32;
        tables.extend([
            (*b"MAXP", profile(count as u32, true)),
            (*b"GLYF", glyf),
            (*b"LOCA", locations(&offsets, long_loca)),
        ]);
    }
    if legacy {
        let glyph = simple_glyph(&[(0, 0, 1), (10, 0, 1), (10, 10, 1)]);
        let end = glyph.len() as u32;
        tables.extend([
            (*b"maxp", profile(3, false)),
            (*b"glyf", glyph),
            (*b"loca", locations(&[0, 0, end, end], long_loca)),
        ]);
    }
    let mut metrics = vec![0; 4 + (count - 1) * 2];
    metrics[..2].copy_from_slice(&1000u16.to_be_bytes());
    tables.extend([
        (
            if extended_metrics { *b"HHEA" } else { *b"hhea" },
            metric_header(1, extended_metrics, false),
        ),
        (if extended_metrics { *b"HMTX" } else { *b"hmtx" }, metrics),
    ]);
    let tables: Vec<_> = tables
        .iter()
        .map(|(tag, bytes)| (*tag, bytes.as_slice()))
        .collect();
    font(&tables)
}
