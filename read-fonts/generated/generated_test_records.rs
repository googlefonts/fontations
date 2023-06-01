// THIS FILE IS AUTOGENERATED.
// Any changes to this file will be overwritten.
// For more information about how codegen works, see font-codegen/README.md

#[allow(unused_imports)]
use crate::codegen_prelude::*;

#[derive(Debug, Clone, Copy)]
#[doc(hidden)]
pub struct BasicTableMarker {
    simple_records_byte_len: usize,
    array_records_byte_len: usize,
}

impl BasicTableMarker {
    fn simple_count_byte_range(&self) -> Range<usize> {
        let start = 0;
        start..start + u16::RAW_BYTE_LEN
    }
    fn simple_records_byte_range(&self) -> Range<usize> {
        let start = self.simple_count_byte_range().end;
        start..start + self.simple_records_byte_len
    }
    fn arrays_inner_count_byte_range(&self) -> Range<usize> {
        let start = self.simple_records_byte_range().end;
        start..start + u16::RAW_BYTE_LEN
    }
    fn array_records_count_byte_range(&self) -> Range<usize> {
        let start = self.arrays_inner_count_byte_range().end;
        start..start + u32::RAW_BYTE_LEN
    }
    fn array_records_byte_range(&self) -> Range<usize> {
        let start = self.array_records_count_byte_range().end;
        start..start + self.array_records_byte_len
    }
}

impl<'a> FontRead<'a> for BasicTable<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        let mut cursor = data.cursor();
        let simple_count: u16 = cursor.read()?;
        let simple_records_byte_len = simple_count as usize * SimpleRecord::RAW_BYTE_LEN;
        cursor.advance_by(simple_records_byte_len);
        let arrays_inner_count: u16 = cursor.read()?;
        let array_records_count: u32 = cursor.read()?;
        let array_records_byte_len = array_records_count as usize
            * <ContainsArrays as ComputeSize>::compute_size(&arrays_inner_count);
        cursor.advance_by(array_records_byte_len);
        cursor.finish(BasicTableMarker {
            simple_records_byte_len,
            array_records_byte_len,
        })
    }
}

pub type BasicTable<'a> = TableRef<'a, BasicTableMarker>;

impl<'a> BasicTable<'a> {
    pub fn simple_count(&self) -> u16 {
        let range = self.shape.simple_count_byte_range();
        self.data.read_at(range.start).unwrap()
    }

    pub fn simple_records(&self) -> &'a [SimpleRecord] {
        let range = self.shape.simple_records_byte_range();
        self.data.read_array(range).unwrap()
    }

    pub fn arrays_inner_count(&self) -> u16 {
        let range = self.shape.arrays_inner_count_byte_range();
        self.data.read_at(range.start).unwrap()
    }

    pub fn array_records_count(&self) -> u32 {
        let range = self.shape.array_records_count_byte_range();
        self.data.read_at(range.start).unwrap()
    }

    pub fn array_records(&self) -> ComputedArray<'a, ContainsArrays<'a>> {
        let range = self.shape.array_records_byte_range();
        self.data
            .read_with_args(range, &self.arrays_inner_count())
            .unwrap()
    }
}

#[cfg(feature = "traversal")]
impl<'a> SomeTable<'a> for BasicTable<'a> {
    fn type_name(&self) -> &str {
        "BasicTable"
    }
    fn get_field(&self, idx: usize) -> Option<Field<'a>> {
        match idx {
            0usize => Some(Field::new("simple_count", self.simple_count())),
            1usize => Some(Field::new(
                "simple_records",
                traversal::FieldType::array_of_records(
                    stringify!(SimpleRecord),
                    self.simple_records(),
                    self.offset_data(),
                ),
            )),
            2usize => Some(Field::new("arrays_inner_count", self.arrays_inner_count())),
            3usize => Some(Field::new(
                "array_records_count",
                self.array_records_count(),
            )),
            4usize => Some(Field::new(
                "array_records",
                traversal::FieldType::computed_array(
                    "ContainsArrays",
                    self.array_records(),
                    self.offset_data(),
                ),
            )),
            _ => None,
        }
    }
}

#[cfg(feature = "traversal")]
impl<'a> std::fmt::Debug for BasicTable<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (self as &dyn SomeTable<'a>).fmt(f)
    }
}

#[derive(Clone, Debug)]
#[repr(C)]
#[repr(packed)]
pub struct SimpleRecord {
    pub val1: BigEndian<u16>,
    pub va2: BigEndian<u32>,
}

impl SimpleRecord {
    pub fn val1(&self) -> u16 {
        self.val1.get()
    }

    pub fn va2(&self) -> u32 {
        self.va2.get()
    }
}

impl FixedSize for SimpleRecord {
    const RAW_BYTE_LEN: usize = u16::RAW_BYTE_LEN + u32::RAW_BYTE_LEN;
}

unsafe impl JustBytes for SimpleRecord {
    fn this_trait_should_only_be_implemented_in_generated_code() {}
}

#[cfg(feature = "traversal")]
impl<'a> SomeRecord<'a> for SimpleRecord {
    fn traverse(self, data: FontData<'a>) -> RecordResolver<'a> {
        RecordResolver {
            name: "SimpleRecord",
            get_field: Box::new(move |idx, _data| match idx {
                0usize => Some(Field::new("val1", self.val1())),
                1usize => Some(Field::new("va2", self.va2())),
                _ => None,
            }),
            data,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ContainsArrays<'a> {
    pub scalars: &'a [BigEndian<u16>],
    pub records: &'a [SimpleRecord],
}

impl<'a> ContainsArrays<'a> {
    pub fn scalars(&self) -> &'a [BigEndian<u16>] {
        self.scalars
    }

    pub fn records(&self) -> &'a [SimpleRecord] {
        self.records
    }
}

impl ReadArgs for ContainsArrays<'_> {
    type Args = u16;
}

impl ComputeSize for ContainsArrays<'_> {
    fn compute_size(args: &u16) -> usize {
        let array_len = *args;
        array_len as usize * u16::RAW_BYTE_LEN + array_len as usize * SimpleRecord::RAW_BYTE_LEN
    }
}

impl<'a> FontReadWithArgs<'a> for ContainsArrays<'a> {
    fn read_with_args(data: FontData<'a>, args: &u16) -> Result<Self, ReadError> {
        let mut cursor = data.cursor();
        let array_len = *args;
        Ok(Self {
            scalars: cursor.read_array(array_len as usize)?,
            records: cursor.read_array(array_len as usize)?,
        })
    }
}

impl<'a> ContainsArrays<'a> {
    /// A constructor that requires additional arguments.
    ///
    /// This type requires some external state in order to be
    /// parsed.
    pub fn read(data: FontData<'a>, array_len: u16) -> Result<Self, ReadError> {
        let args = array_len;
        Self::read_with_args(data, &args)
    }
}

#[cfg(feature = "traversal")]
impl<'a> SomeRecord<'a> for ContainsArrays<'a> {
    fn traverse(self, data: FontData<'a>) -> RecordResolver<'a> {
        RecordResolver {
            name: "ContainsArrays",
            get_field: Box::new(move |idx, _data| match idx {
                0usize => Some(Field::new("scalars", self.scalars())),
                1usize => Some(Field::new(
                    "records",
                    traversal::FieldType::array_of_records(
                        stringify!(SimpleRecord),
                        self.records(),
                        _data,
                    ),
                )),
                _ => None,
            }),
            data,
        }
    }
}

#[derive(Clone, Debug)]
#[repr(C)]
#[repr(packed)]
pub struct ContainsOffests {
    pub off_array_count: BigEndian<u16>,
    pub array_offset: BigEndian<Offset16>,
    pub other_offset: BigEndian<Offset32>,
}

impl ContainsOffests {
    pub fn off_array_count(&self) -> u16 {
        self.off_array_count.get()
    }

    pub fn array_offset(&self) -> Offset16 {
        self.array_offset.get()
    }

    /// Attempt to resolve [`array_offset`][Self::array_offset].
    pub fn array<'a>(&self, data: FontData<'a>) -> Result<&'a [SimpleRecord], ReadError> {
        let args = self.off_array_count();
        self.array_offset().resolve_with_args(data, &args)
    }

    pub fn other_offset(&self) -> Offset32 {
        self.other_offset.get()
    }

    /// Attempt to resolve [`other_offset`][Self::other_offset].
    pub fn other<'a>(&self, data: FontData<'a>) -> Result<BasicTable<'a>, ReadError> {
        self.other_offset().resolve(data)
    }
}

impl FixedSize for ContainsOffests {
    const RAW_BYTE_LEN: usize = u16::RAW_BYTE_LEN + Offset16::RAW_BYTE_LEN + Offset32::RAW_BYTE_LEN;
}

unsafe impl JustBytes for ContainsOffests {
    fn this_trait_should_only_be_implemented_in_generated_code() {}
}

#[cfg(feature = "traversal")]
impl<'a> SomeRecord<'a> for ContainsOffests {
    fn traverse(self, data: FontData<'a>) -> RecordResolver<'a> {
        RecordResolver {
            name: "ContainsOffests",
            get_field: Box::new(move |idx, _data| match idx {
                0usize => Some(Field::new("off_array_count", self.off_array_count())),
                1usize => Some(Field::new(
                    "array_offset",
                    traversal::FieldType::offset_to_array_of_records(
                        self.array_offset(),
                        self.array(_data),
                        stringify!(SimpleRecord),
                        _data,
                    ),
                )),
                2usize => Some(Field::new(
                    "other_offset",
                    FieldType::offset(self.other_offset(), self.other(_data)),
                )),
                _ => None,
            }),
            data,
        }
    }
}
