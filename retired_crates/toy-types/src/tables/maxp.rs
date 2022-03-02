use crate::*;
//Version 0.5
#[derive(Clone, Debug, FontThing)]
#[allow(dead_code)]
pub struct Maxp05 {
    pub version: Version16Dot16,
    pub num_glyphs: uint16,
}

//Version 1.0
#[derive(Clone, Debug, FontThing)]
#[allow(dead_code)]
pub struct Maxp10 {
    pub version: Version16Dot16,
    pub num_glyphs: uint16,
    pub max_points: uint16,
    pub max_contours: uint16,
    pub max_composite_points: uint16,
    pub max_composite_contours: uint16,
    pub max_zones: uint16,
    pub max_twilight_points: uint16,
    pub max_storage: uint16,
    pub max_function_defs: uint16,
    pub max_instruction_defs: uint16,
    pub max_stack_elements: uint16,
    pub max_size_of_instructions: uint16,
    pub max_component_elements: uint16,
    pub max_component_depth: uint16,
}
