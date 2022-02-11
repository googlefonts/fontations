toy_table_macro::tables! {
    TooMuchCount<'a> {
        item_count: Uint16,
        #[count(item_count)]
        #[count_with(double, item_count)]
        items: [Uint16],
    }
}

fn double(val: raw_types::Uint16) -> usize {
    val.get() as usize * 2
}

toy_table_macro::tables! {
    BadCountFn<'a> {
        item_count: Uint16,
        #[count_with(double, Self::item_count)]
        items: [Uint16],
    }
}

toy_table_macro::tables! {
    MissingCount<'a> {
        item_count: Uint24,
        items: [Uint24],
    }
}

toy_table_macro::tables! {
    CountOnScalar<'a> {
        #[count(item_count)]
        item_count: Uint24,
        #[count(item_count)]
        items: [Uint24],
    }
}

toy_table_macro::tables! {
    HiddenOnArray<'a> {
        item_count: Uint24,
        #[hidden]
        #[count(item_count)]
        items: [Uint24],
    }
}


fn main() {}
