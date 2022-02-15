use raw_types::Uint16;

toy_table_macro::tables! {
    CountAll<'a> {
        item_count: Uint16,
        #[count_all]
        items: [Uint16],
        other: Uint16,
    }
}

fn main() {}
