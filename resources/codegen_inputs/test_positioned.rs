#![parse_module(read_fonts::codegen_test::positioned)]

// A record read at an offset within the enclosing table, rather than from data
// sliced to it. Hand-written in `read-fonts/src/codegen_test.rs`.
//
// It holds an offset that is measured from the start of the enclosing table,
// which is what forces the arrangement: slicing to the record would leave
// nothing to resolve that offset against.
extern record Positioned<'a>;

/// Holds a positioned record directly, and an array of records that hold them.
#[skip_constructor]
table PositionedTable {
    /// The size of each [Positioned] record in this table, in bytes.
    positioned_size: u16,
    /// A positioned record read straight from a table field.
    #[read_with($positioned_size)]
    solo: Positioned,
    #[compile(array_len($pairs))]
    pair_count: u16,
    /// An array of records that are positioned by containment.
    #[count($pair_count)]
    #[read_with($positioned_size)]
    pairs: ComputedArray<PositionedPair<'a>>,
}

/// Positioned because it contains positioned fields.
#[read_args(positioned_size: u16)]
#[skip_constructor]
record PositionedPair<'a> {
    /// A plain scalar, to check the following fields are still located
    /// correctly.
    tag: u16,
    #[read_with($positioned_size)]
    first: Positioned,
    #[read_with($positioned_size)]
    second: Positioned,
}

/// Exercises the nesting case: a table of groups, each holding an array of
/// records that are themselves positioned.
#[skip_constructor]
table NestedPositionedTable {
    /// The size of each [Positioned] record in this table, in bytes.
    positioned_size: u16,
    /// The number of pairs in every group.
    pairs_per_group: u16,
    #[compile(array_len($groups))]
    group_count: u16,
    #[count($group_count)]
    #[read_with($pairs_per_group, $positioned_size)]
    groups: ComputedArray<PositionedGroup<'a>>,
}

/// Positioned only because it contains an array of positioned records; it has
/// no positioned field of its own.
#[read_args(pairs_per_group: u16, positioned_size: u16)]
#[skip_constructor]
record PositionedGroup<'a> {
    #[count($pairs_per_group)]
    #[read_with($positioned_size)]
    pairs: ComputedArray<PositionedPair<'a>>,
}
