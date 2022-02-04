use toy_types_derive::FontThing;
use toy_types::*;

#[derive(FontThing)]
struct ArrayAll<'a> {
    one: u16,
    two: u16,
    #[font_thing(all)]
    items: Array<'a, u8>,
}

fn main() {
    let mut buf = Vec::new();
    buf.extend(3_u16.to_be_bytes());
    buf.extend(5_u16.to_be_bytes());
    buf.extend(b"this is 15 chrs");
    let blob = Blob::new(&buf);
    let test = ArrayAll::read(blob).unwrap();
    assert_eq!(test.one, 3);
    assert_eq!(test.two, 5);
    let bytes = test.items.iter().collect::<Vec<_>>();
    assert_eq!(bytes, b"this is 15 chrs");
}
