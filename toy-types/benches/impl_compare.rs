use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
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
    fn pod_get_bbox(font: &FontRef, gid: u16) -> Option<Bbox> {
        let head = font.head().expect("missing head");
        let _32bit_loca = head.index_to_loc_format == 1;
        let loca = font.loca(_32bit_loca).expect("missing loca");
        let glyf = font.glyf().expect("missing glyf");
        let g_off = loca.get(gid as usize);
        g_off
            .and_then(|off| glyf.get(off as usize))
            .map(|glyph| Bbox {
                x0: glyph.x_min,
                x1: glyph.x_max,
                y0: glyph.y_min,
                y1: glyph.y_max,
            })
    }

    let data = get_font_bytes();
    let font = FontRef::new(&data).unwrap();
    let head = font.head().expect("missing head");
    let _32bit_loca = head.index_to_loc_format == 1;
    let loca = font.loca(_32bit_loca).expect("missing loca");
    let glyf = font.glyf().expect("missing glyf");
    let offset = loca.get(10).unwrap();

    c.bench_with_input(
        BenchmarkId::new("pod_glyph_bbox_only", offset),
        &offset,
        |b, i| b.iter(|| glyf.get(*i as usize).map(|g| g.x_max - g.x_min)),
    );
    c.bench_function("pod_glyph_bbox_from_root 1", |b| {
        b.iter(|| pod_get_bbox(&font, 10))
    });
    c.bench_function("pod_glyph_bbox_from_root 1000", |b| {
        b.iter(|| (0u16..=1000).map(|gid| pod_get_bbox(&font, gid)))
    });
}

pub fn view_glyph_bbox(c: &mut Criterion) {
    fn view_get_bbox(font: &FontRef, gid: u16) -> Option<Bbox> {
        let head = font.head_ref().expect("missing head");
        let _32bit_loca = head.index_to_loc_format()? == 1;
        let loca = font.loca(_32bit_loca).expect("missing loca");
        let glyf = font.glyf().expect("missing glyf");
        let g_off = loca.get(gid as usize);
        g_off
            .and_then(|off| glyf.get_view(off as usize))
            .map(|glyph| Bbox {
                x0: glyph.x_min().unwrap_or(0),
                x1: glyph.x_max().unwrap_or(0),
                y0: glyph.y_min().unwrap_or(0),
                y1: glyph.y_max().unwrap_or(0),
            })
    }

    let data = get_font_bytes();
    let font = FontRef::new(&data).unwrap();
    let head = font.head().expect("missing head");
    let _32bit_loca = head.index_to_loc_format == 1;
    let loca = font.loca(_32bit_loca).expect("missing loca");
    let glyf = font.glyf().expect("missing glyf");
    let offset = loca.get(10).unwrap();

    c.bench_with_input(
        BenchmarkId::new("view_glyph_bbox_from_glyf 1", offset),
        &offset,
        |b, i| {
            b.iter(|| {
                glyf.get_view(*i as usize)
                    .map(|g| g.x_max().unwrap_or_default() - g.x_min().unwrap_or_default())
            })
        },
    );

    c.bench_function("view_glyph_bbox_from_root 1", |b| {
        b.iter(|| view_get_bbox(&font, 10))
    });
    c.bench_function("view_glyph_bbox_from_root 1000", |b| {
        b.iter(|| (0u16..=1000).map(|gid| view_get_bbox(&font, gid)))
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

#[allow(dead_code)]
struct Bbox {
    x0: i16,
    x1: i16,
    y0: i16,
    y1: i16,
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
