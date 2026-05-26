// This file tests codegen for generic groups: a table with a generic offset
// type parameter and a group enum that dispatches based on a type field.
//
// Based on the Lookup / PositionLookup pattern in GPOS.

#![parse_module(read_fonts::codegen_test::generic_group)]

/// A generic table parameterized by the type of its subtable offsets.
#[generic_offset(T)]
#[skip_constructor]
table MyLookup {
    /// Determines the concrete type of T
    lookup_type: u16,
    /// Number of subtables
    #[compile(array_len($subtable_offsets))]
    sub_table_count: u16,
    /// Offsets to subtables, from beginning of this table
    #[count($sub_table_count)]
    subtable_offsets: [Offset16<T>],
}

/// A concrete subtable with a format field (format group)
format u16 MySubtable {
    Format1(MySubtableFormat1),
    Format2(MySubtableFormat2),
}

#[skip_constructor]
table MySubtableFormat1 {
    #[format = 1]
    format: u16,
    value: u16,
}

#[skip_constructor]
table MySubtableFormat2 {
    #[format = 2]
    format: u16,
    #[compile(array_len($values))]
    count: u16,
    #[count($count)]
    values: [u16],
}

/// The group enum dispatching on lookup_type.
group MyLookupGroup(MyLookup, $lookup_type) {
    1 => TypeOne(MySubtable),
    2 => TypeTwo(MySubtableFormat1),
}

#[skip_constructor]
table ContainsLookupGroup {
    version: u16,
    lookup_offset: Offset16<MyLookupGroup>,
}
