use criterion::{criterion_group, criterion_main, Criterion};
use toy_types::tables::{Cmap, Cmap4, Cmap4Zero, FontRef, TableProvider, TableProviderRef};

fn get_font_bytes() -> Vec<u8> {
    std::fs::read("/Users/rofls/Library/Fonts/Inconsolata-Regular.ttf").unwrap()
}

pub fn pod_get_head_fields(c: &mut Criterion) {
    fn our_impl(font: &FontRef) -> Option<(u16, i16)> {
        // upm & loca_format
        font.head()
            .map(|head| (head.units_per_em, head.index_to_loc_format))
    }

    let data = get_font_bytes();
    let font = FontRef::new(&data).unwrap();
    c.bench_function("pod_get_head_fields", |b| b.iter(|| our_impl(&font)));
}

pub fn view_get_head_fields(c: &mut Criterion) {
    fn our_impl(font: &FontRef) -> Option<(u16, i16)> {
        // upm & loca_format
        font.head_ref().map(|head| {
            (
                head.units_per_em().unwrap_or(42),
                head.index_to_loc_format().unwrap_or(-1),
            )
        })
    }
    let data = get_font_bytes();
    let font = FontRef::new(&data).unwrap();
    c.bench_function("view_get_head_fields", |b| b.iter(|| our_impl(&font)));
}

pub fn zc_get_head_fields(c: &mut Criterion) {
    fn our_impl(font: &FontRef) -> Option<(u16, i16)> {
        // upm & loca_format
        font.head_zero()
            .map(|head| (head.units_per_em.get(), head.index_to_loc_format.get()))
    }
    let data = get_font_bytes();
    let font = FontRef::new(&data).unwrap();
    c.bench_function("zc_get_head_fields", |b| b.iter(|| our_impl(&font)));
}

pub fn pod_glyph_bbox(c: &mut Criterion) {
    fn pod_get_bbox_area(font: &FontRef, gid: u16) -> usize {
        let head = font.head().expect("missing head");
        let _32bit_loca = head.index_to_loc_format == 1;
        let loca = font.loca(_32bit_loca).expect("missing loca");
        let glyf = font.glyf().expect("missing glyf");
        let g_off = loca.get(gid as usize);
        g_off
            .and_then(|off| glyf.get(off as usize))
            .map(|glyph| {
                (glyph.x_max - glyph.x_min) as usize * (glyph.y_max - glyph.y_min) as usize
            })
            .unwrap_or_default()
    }

    let data = get_font_bytes();
    let font = FontRef::new(&data).unwrap();
    let head = font.head().expect("missing head");
    let _32bit_loca = head.index_to_loc_format == 1;
    let loca = font.loca(_32bit_loca).expect("missing loca");
    let glyf = font.glyf().expect("missing glyf");
    let offset = loca.get(10).unwrap();

    c.bench_function("pod_glyph_bbox_only", |b| {
        b.iter(|| glyf.get(offset as usize).map(|g| g.x_max - g.x_min))
    });
    c.bench_function("zc_glyph_bbox_only", |b| {
        b.iter(|| {
            glyf.get_zc(offset as usize)
                .map(|g| g.x_max.get() - g.x_min.get())
        })
    });
    c.bench_function("pod_glyph_bbox_root", |b| {
        b.iter(|| pod_get_bbox_area(&font, 10))
    });
}

pub fn view_glyph_bbox(c: &mut Criterion) {
    fn view_get_bbox_area(font: &FontRef, gid: u16) -> usize {
        let head = font.head_ref().expect("missing head");
        let _32bit_loca = head.index_to_loc_format() == Some(1);
        let loca = font.loca(_32bit_loca).expect("missing loca");
        let glyf = font.glyf().expect("missing glyf");
        let g_off = loca.get(gid as usize);
        g_off
            .and_then(|off| glyf.get_view(off as usize))
            .map(|glyph| {
                (glyph.x_max().unwrap_or(0) - glyph.x_min().unwrap_or(0)) as usize
                    * (glyph.y_max().unwrap_or(0) - glyph.y_min().unwrap_or(0)) as usize
            })
            .unwrap_or_default()
    }

    let data = get_font_bytes();
    let font = FontRef::new(&data).unwrap();
    let head = font.head().expect("missing head");
    let _32bit_loca = head.index_to_loc_format == 1;
    let loca = font.loca(_32bit_loca).expect("missing loca");
    let glyf = font.glyf().expect("missing glyf");
    let offset = loca.get(10).unwrap();

    c.bench_function("view_glyph_bbox_only", |b| {
        b.iter(|| {
            glyf.get_view(offset as usize)
                .map(|g| g.x_max().unwrap_or_default() - g.x_min().unwrap_or_default())
        })
    });
    c.bench_function("view_glyph_bbox_root", |b| {
        b.iter(|| view_get_bbox_area(&font, 10))
    });
}

pub fn pod_cmap_lookup(c: &mut Criterion) {
    fn retain_subtable(cmap: &Cmap4) -> usize {
        (cmap.glyph_id_for_char('\u{2}') + cmap.glyph_id_for_char('A')) as usize
    }

    fn get_subtable(cmap: &Cmap, subtable_offset: u32) -> usize {
        let cmap4 = cmap.parse_subtable::<Cmap4>(subtable_offset).unwrap();
        (cmap4.glyph_id_for_char('\u{2}') + cmap4.glyph_id_for_char('A')) as usize
    }

    let data = get_font_bytes();
    let font = FontRef::new(&data).unwrap();
    let cmap = font.cmap().unwrap();
    let subtable_offset = cmap
        .encoding_records
        .iter()
        .find(|record| cmap.get_subtable_version(record.subtable_offset) == Some(4))
        .map(|record| record.subtable_offset)
        .expect("failed to load cmap table");

    let cmap4 = cmap.parse_subtable::<Cmap4>(subtable_offset).unwrap();

    c.bench_function("pod_cmap_lookup_retain", |b| {
        b.iter(|| retain_subtable(&cmap4))
    });
    c.bench_function("pod_cmap_lookup_get", |b| {
        b.iter(|| get_subtable(&cmap, subtable_offset))
    });
}

pub fn zc_cmap_lookup(c: &mut Criterion) {
    fn retain_subtable(cmap4: &Cmap4Zero) -> usize {
        (cmap4.glyph_id_for_char('\u{2}') + cmap4.glyph_id_for_char('A')) as usize
    }

    fn get_subtable(cmap: &Cmap, subtable_offset: u32) -> usize {
        let cmap4 = cmap.get_zerocopy_cmap4(subtable_offset).unwrap();
        (cmap4.glyph_id_for_char('\u{2}') + cmap4.glyph_id_for_char('A')) as usize
    }

    let data = get_font_bytes();
    let font = FontRef::new(&data).unwrap();
    let cmap = font.cmap().unwrap();
    let subtable_offset = cmap
        .encoding_records
        .iter()
        .find(|record| cmap.get_subtable_version(record.subtable_offset) == Some(4))
        .map(|record| record.subtable_offset)
        .expect("failed to load cmap table");

    let cmap4 = cmap.get_zerocopy_cmap4(subtable_offset).unwrap();

    c.bench_function("zc_cmap_lookup_retain", |b| {
        b.iter(|| retain_subtable(&cmap4))
    });
    c.bench_function("zc_cmap_lookup_get", |b| {
        b.iter(|| get_subtable(&cmap, subtable_offset))
    });
}

criterion_group!(cmap_lookup, pod_cmap_lookup, zc_cmap_lookup);
criterion_group!(
    get_head_fields,
    pod_get_head_fields,
    view_get_head_fields,
    zc_get_head_fields
);
criterion_group!(glyf_bbox, pod_glyph_bbox, view_glyph_bbox);

criterion_main!(cmap_lookup, get_head_fields, glyf_bbox);
