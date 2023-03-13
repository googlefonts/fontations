// This file tests the generation of bitflags.

#![parse_module(read_fonts::codegen_test::enums)]

enum u16 MyEnum1 {
    /// doc me baby
    /// docington bear
    #[default]
    ItsAZero = 0,
    ItsAOne = 1,
}

enum u16 MyEnum2 {    
    ItsATwo = 2,
    /// A very important three
    #[default]
    ItsAThree = 3,
}

record MyRecord {
    my_enum1: MyEnum1,
    my_enum2: MyEnum2,
}