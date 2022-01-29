use font_types_macro::FontThing;
use toy_types::FromBeBytes;

#[derive(FontThing)]
struct Durp {
    version_minor: u16,
    version_major: u16,
    durp: i32,
    offset: u16,
    n_items: u16,
    #[font_thing(count = "n_items", offset = "offset")]
    items: Vec<u16>,
}

fn main() {
    let mut buffer = Vec::new();
    buffer.extend(420u16.to_be_bytes());
    buffer.extend(1u16.to_be_bytes());
    buffer.extend((-6i32).to_be_bytes());
    buffer.extend(3u16.to_be_bytes());
    buffer.extend(13u16.to_be_bytes());

    let herp = Durp::from_bytes(&buffer).unwrap();
    assert_eq!(herp.durp, -6);
    assert_eq!(herp.offset, 3);
}
