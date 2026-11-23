//! The [cpal](https://learn.microsoft.com/en-us/typography/opentype/spec/cpal) table

include!("../../generated/generated_cpal.rs");

impl Cpal {
    fn compute_version(&self) -> u16 {
        // Using v1-only fields?
        if self.palette_types_array.is_some()
            || self.palette_labels_array.is_some()
            || self.palette_entry_labels_array.is_some()
        {
            return 1;
        }
        0
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn make_simple_cpal(palettes: Vec<Vec<(u8, u8, u8, u8)>>) -> Cpal {
        let mut cpal = Cpal {
            num_palettes: palettes.len() as u16,
            num_palette_entries: palettes[0].len() as u16,
            num_color_records: palettes.iter().map(Vec::len).sum::<usize>() as u16,
            color_record_indices: (0..palettes.len())
                .map(|i| (i * palettes[0].len()) as u16)
                .collect(),
            ..Default::default()
        };
        cpal.color_records_array.set(
            palettes
                .iter()
                .flat_map(|p| p.iter())
                .copied()
                .map(|(r, g, b, a)| ColorRecord {
                    red: r,
                    green: g,
                    blue: b,
                    alpha: a,
                })
                .collect::<Vec<_>>(),
        );
        cpal
    }

    #[test]
    fn write_read_simple_cpal() {
        let cpal = make_simple_cpal(vec![
            vec![(0, 0, 255, 255), (255, 0, 0, 255)],
            vec![(0, 255, 0, 255), (255, 0, 255, 255)],
            vec![(0, 0, 0, 255), (128, 0, 128, 255)],
        ]);
        let bytes = crate::write::dump_table(&cpal).unwrap();
        let loaded = read_fonts::tables::cpal::Cpal::read(FontData::new(&bytes)).unwrap();
        assert_eq!(
            (2, 3, 6),
            (
                loaded.num_palette_entries(),
                loaded.num_palettes(),
                loaded.num_color_records()
            )
        );

        let loaded_colors = loaded.color_records_array().unwrap().unwrap();
        assert_eq!(
            vec![
                read_fonts::tables::cpal::ColorRecord {
                    red: 0,
                    green: 255,
                    blue: 0,
                    alpha: 255,
                },
                read_fonts::tables::cpal::ColorRecord {
                    red: 128,
                    green: 0,
                    blue: 128,
                    alpha: 255,
                },
            ],
            vec![loaded_colors[2], *loaded_colors.last().unwrap(),],
        );
        assert_eq!(loaded.version(), 0);
    }

    #[test]
    fn write_read_cpal_v1() {
        let mut cpal = make_simple_cpal(vec![
            vec![(0, 0, 0, 255), (255, 0, 0, 255)],
            vec![(255, 255, 255, 255), (0, 255, 255, 255)],
        ]);
        cpal.palette_types_array = Some(vec![
            PaletteType::USABLE_WITH_LIGHT_BACKGROUND,
            PaletteType::USABLE_WITH_DARK_BACKGROUND,
        ])
        .into();
        cpal.palette_labels_array = Some(vec![NameId::new(256), NameId::new(257)]).into();
        cpal.palette_entry_labels_array = Some(vec![NameId::new(258), NameId::new(0xFFFF)]).into();

        let bytes = crate::write::dump_table(&cpal).unwrap();
        let loaded = read_fonts::tables::cpal::Cpal::read(FontData::new(&bytes)).unwrap();
        assert_eq!(loaded.version(), 1);
        assert_eq!(
            loaded
                .palette_types_array()
                .unwrap()
                .unwrap()
                .iter()
                .map(|t| t.get())
                .collect::<Vec<_>>(),
            vec![
                PaletteType::USABLE_WITH_LIGHT_BACKGROUND,
                PaletteType::USABLE_WITH_DARK_BACKGROUND,
            ]
        );
        assert_eq!(
            loaded
                .palette_labels_array()
                .unwrap()
                .unwrap()
                .iter()
                .map(|id| id.get())
                .collect::<Vec<_>>(),
            vec![NameId::new(256), NameId::new(257)]
        );
        assert_eq!(
            loaded
                .palette_entry_labels_array()
                .unwrap()
                .unwrap()
                .iter()
                .map(|id| id.get())
                .collect::<Vec<_>>(),
            vec![NameId::new(258), NameId::new(0xFFFF)]
        );
        // and the owned table survives a round trip
        let owned: Cpal = loaded.to_owned_table();
        assert_eq!(owned, cpal);
    }
}
