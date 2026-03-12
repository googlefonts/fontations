use std::env;
use std::fs;
use read_fonts::{FontRef, TableProvider, FontData, FontRead};
use read_fonts::tables::ift::{Ift, EntryFormatFlags};
use read_fonts::types::{Uint24, Int24};
use read_fonts::collections::int_set::IntSet;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: ift_dump <font_path>");
        std::process::exit(1);
    }

    let font_path = &args[1];
    let font_bytes = fs::read(font_path).expect("failed to read font file");
    let font = FontRef::new(&font_bytes).expect("failed to parse font");

    if let Ok(ift) = font.ift() {
        println!("IFT table found:");
        dump_ift(ift);
        println!();
    } else {
        println!("IFT table not found.");
    }

    if let Ok(iftx) = font.iftx() {
        println!("IFTX table found:");
        dump_ift(iftx);
        println!();
    } else {
        println!("IFTX table not found.");
    }
}

fn dump_ift(ift: Ift) {
    match ift {
        Ift::Format1(f1) => {
            println!("  Format: 1");
            println!("  Compatibility ID: {:?}", f1.compatibility_id().as_slice());
            println!("  URL Template: {}", String::from_utf8_lossy(f1.url_template()));
            println!("  Patch Format: {}", f1.patch_format());
            println!("  Max Entry Index: {}", f1.max_entry_index());
            println!("  Glyph Count: {}", f1.glyph_count());

            if let Ok(glyph_map) = f1.glyph_map() {
                println!("  Glyph Map:");
                println!("    First Mapped Glyph: {}", glyph_map.first_mapped_glyph());
            }

            if let Some(feature_map_result) = f1.feature_map() {
                match feature_map_result {
                    Ok(feature_map) => {
                        println!("  Feature Map:");
                        println!("    Feature Count: {}", feature_map.feature_count());
                    }
                    Err(e) => println!("    Error reading feature map: {:?}", e),
                }
            }
        }
        Ift::Format2(f2) => {
            println!("  Format: 2");
            println!("  Compatibility ID: {:?}", f2.compatibility_id().as_slice());
            println!("  URL Template: {}", String::from_utf8_lossy(f2.url_template()));
            println!("  Default Patch Format: {}", f2.default_patch_format());
            let entry_count = f2.entry_count().to_u32();
            println!("  Entry Count: {}", entry_count);

            let has_string_data = !f2.entry_id_string_data_offset().is_null();

            if let Ok(entries) = f2.entries() {
                println!("  Mapping Entries:");
                let mut data = entries.entry_data();
                for i in 0..entry_count {
                    if data.is_empty() {
                        println!("    (Reached end of data before entry count)");
                        break;
                    }
                    match read_fonts::tables::ift::EntryData::read(FontData::new(data)) {
                        Ok(entry) => {
                            let flags = entry.format_flags();
                            print!("    Entry {}: flags {:08b}", i, flags.bits());
                            if flags.contains(EntryFormatFlags::FEATURES_AND_DESIGN_SPACE) {
                                print!(" [Feat/DS]");
                            }
                            if flags.contains(EntryFormatFlags::CHILD_INDICES) {
                                print!(" [Child]");
                            }
                            if flags.contains(EntryFormatFlags::ENTRY_ID_DELTA) {
                                print!(" [ID Delta]");
                            }
                            if flags.contains(EntryFormatFlags::PATCH_FORMAT) {
                                print!(" [Format]");
                            }
                            if flags.contains(EntryFormatFlags::CODEPOINTS_BIT_1) || flags.contains(EntryFormatFlags::CODEPOINTS_BIT_2) {
                                print!(" [CP]");
                            }
                            println!();

                            // Manually parse trailing data to find where the next entry starts
                            let mut trailing = entry.trailing_data();
                            
                            // 1. Entry ID Delta
                            if flags.contains(EntryFormatFlags::ENTRY_ID_DELTA) {
                                loop {
                                    if trailing.len() < 3 { break; }
                                    let has_more = if has_string_data {
                                        let val: Uint24 = FontData::new(trailing).read_at(0).unwrap();
                                        (val.to_u32() & (1 << 23)) > 0
                                    } else {
                                        let val: Int24 = FontData::new(trailing).read_at(0).unwrap();
                                        (val.to_i32() & 1) > 0
                                    };
                                    trailing = &trailing[3..];
                                    if !has_more { break; }
                                }
                            }

                            // 2. Patch Format
                            if flags.contains(EntryFormatFlags::PATCH_FORMAT) {
                                if !trailing.is_empty() {
                                    trailing = &trailing[1..];
                                }
                            }

                            // 3. Codepoints
                            let cp_flags = flags.bits() & (EntryFormatFlags::CODEPOINTS_BIT_1.bits() | EntryFormatFlags::CODEPOINTS_BIT_2.bits());
                            if cp_flags != 0 {
                                let (bias, skipped) = if cp_flags == EntryFormatFlags::CODEPOINTS_BIT_2.bits() {
                                    (FontData::new(trailing).read_at::<u16>(0).unwrap_or(0) as u32, 2)
                                } else if cp_flags == (EntryFormatFlags::CODEPOINTS_BIT_1.bits() | EntryFormatFlags::CODEPOINTS_BIT_2.bits()) {
                                    (FontData::new(trailing).read_at::<Uint24>(0).map(|v| v.to_u32()).unwrap_or(0), 3)
                                } else {
                                    (0, 0)
                                };
                                
                                if trailing.len() >= skipped {
                                    let cp_data = &trailing[skipped..];
                                    match IntSet::<u32>::from_sparse_bit_set_bounded(cp_data, bias, 0x10FFFF) {
                                        Ok((_, remaining)) => {
                                            trailing = remaining;
                                        }
                                        Err(_) => {
                                            println!("      Error parsing codepoints");
                                        }
                                    }
                                }
                            }

                            // Advance data to next entry
                            let entry_header_len = entry.min_byte_range().end - entry.trailing_data().len();
                            let consumed_trailing = entry.trailing_data().len() - trailing.len();
                            let total_entry_len = entry_header_len + consumed_trailing;
                            data = &data[total_entry_len..];
                        }
                        Err(e) => {
                            println!("    Error reading entry {}: {:?}", i, e);
                            break;
                        }
                    }
                }
            }

            if let Some(id_string_data_result) = f2.entry_id_string_data() {
                match id_string_data_result {
                    Ok(id_string_data) => {
                        println!("  Entry ID String Data Length: {}", id_string_data.id_data().len());
                    }
                    Err(e) => println!("    Error reading entry ID string data: {:?}", e),
                }
            }
        }
    }
}
