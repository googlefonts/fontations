// This file tests the generation of count(..) arrays with element size > 1.

#![parse_module(read_fonts::codegen_test::count_all)]
#![sanitize]

table CountAll16 {
    some_field: u16,
    #[count(..)]
    #[sanitize_with(sanitize_remainder)]
    remainder: [u16]
}

table CountAll32 {
    some_field: u16,
    #[sanitize_with(sanitize_remainder)]
    #[count(..)]
    remainder: [u32]
}
