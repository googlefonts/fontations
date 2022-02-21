toy_table_macro::tables! {
    SingleLifetimeOnly<'a, 'b> {
        item_count: BigEndian<u16>,
        #[count(item_count)]
        items: [BigEndian<Uint24>],
    }
}

fn main() {}
