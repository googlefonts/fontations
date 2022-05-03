font_types_macro::tables! {
    CountAll<'a> {
        item_count: BigEndian<u16>,
        #[count_all]
        items: [BigEndian<u16>],
        other: BigEndian<u16>,
    }
}

fn main() {}
