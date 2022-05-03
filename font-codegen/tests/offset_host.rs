use font_types::{OffsetHost, FontRead, test_helpers::BeBuffer, BigEndian, Offset16};

font_types_macro::tables! {
    #[offset_host]
    MyTable<'a> {
        count: BigEndian<u16>,
        #[count(count)]
        offsets: [BigEndian<Offset16>],
    }
}

fn main() {
    let mut buf = BeBuffer::new();
    buf.push(2_u16);
    buf.extend([Offset16::new(6), Offset16::new(10)]);
    buf.extend([42u32, 1337]);

    let table = MyTable::read(&buf).unwrap();
    let one: BigEndian<u32> = table.offsets().get(0).and_then(|off| table.resolve_offset(off.get())).unwrap();
    let two: BigEndian<u32> = table.offsets().get(1).and_then(|off| table.resolve_offset(off.get())).unwrap();
    assert_eq!(one.get(), 42);
    assert_eq!(two.get(), 1337);

}
