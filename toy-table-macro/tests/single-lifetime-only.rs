toy_table_macro::tables! {
    SingleLifetimeOnly<'a, 'b> {
        item_count: Uint24,
        #[count(item_count)]
        items: [Uint24],
    }
}

toy_table_macro::tables! {
    NoLifetimeBounds<'a: 'b> {
        item_count: Uint24,
        #[count(item_count)]
        items: [Uint24],
    }
}

fn main() {}
