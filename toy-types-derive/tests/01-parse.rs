use toy_types_derive::FontThing;
use toy_types::*;

#[derive(FontThing)]
struct Durp<'font> {
    version_minor: u16,
    version_major: u16,
    durp: i32,
    offset: u16,
    n_items: u16,
    #[font_thing(count = "n_items")]
    items: Array<'font, u16>,
}

fn main() {
    let mut buffer = Vec::new();
    buffer.extend(420u16.to_be_bytes());
    buffer.extend(1u16.to_be_bytes());
    buffer.extend((-6i32).to_be_bytes());
    buffer.extend(12u16.to_be_bytes());
    buffer.extend(5u16.to_be_bytes());
    [1, 2, 3, 4, 5].into_iter().map(u16::to_be_bytes).for_each(|b| buffer.extend(b));
    let blob = toy_types::Blob::new(&buffer);

    let herp = Durp::read(blob).unwrap();
    assert_eq!(herp.durp, -6);
    assert_eq!(herp.offset, 12);
    assert_eq!(herp.items.get(0), Some(1), "{:?}", &buffer);
    assert_eq!(herp.items.get(4), Some(5));
    //dbg!(buffer)
}
