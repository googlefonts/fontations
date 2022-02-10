toy_table_macro::tables! {
// this should fail because it needs to have a lifetime
    MyTable {
        item_count: Uint24,
        #[count(item_count)]
        items: [Uint24],
    }
}


toy_table_macro::tables! {
// this should fail because it needs to *not* a lifetime
    MyTable<'a> {
        item_count: Uint24,
        item_size: Int16,

    }
}

fn main() {}
