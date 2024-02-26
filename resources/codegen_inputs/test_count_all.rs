// This file tests the generation of count(..) arrays with element size > 1.

#![parse_module(read_fonts::codegen_test::count_all)]

table CountAll16 {
    some_field: u16,
    #[count(..)]
    remainder: [u16]
}

table CountAll32 {
    some_field: u16,
    #[count(..)]
    remainder: [u32]
}
