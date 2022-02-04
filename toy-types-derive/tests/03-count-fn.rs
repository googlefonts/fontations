use toy_types_derive::FontThing;
use toy_types::*;

#[derive(FontThing)]
struct CountTest<'a> {
    count_one: u16,
    count_two: u16,
    #[font_thing(count(fn = "std::ops::Mul::mul", args("count_one", "count_two")))]
    //NOTE: lots of options with syntax; not using a string would let us not have
    //syn's 'parsing' feature enabled
    //#[font_thing(count(std::ops::Mul::mul, args("count_one", "count_two")))]
    items: Array<'a, u8>,
    word: u32,
}

fn main() {
    let mut buf = Vec::new();
    buf.extend(3_u16.to_be_bytes());
    buf.extend(5_u16.to_be_bytes());
    buf.extend(b"this is 15 chrs");
    buf.extend(0xca11_c0c0_u32.to_be_bytes());
    let blob = Blob::new(&buf);
    let test = CountTest::read(blob).unwrap();
    assert_eq!(test.count_one, 3);
    assert_eq!(test.count_two, 5);
    let bytes = test.items.iter().collect::<Vec<_>>();
    assert_eq!(bytes, b"this is 15 chrs");
    assert_eq!(test.word, 0xca11_c0c0);
}
