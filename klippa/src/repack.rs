//! Implement repacking algorithm to resolve offset overflows
//! ref:<https://github.com/harfbuzz/harfbuzz/blob/9cfb0e6786ecceabaec7a26fd74b1ddb1209f74d/src/hb-repacker.hh#L454>

use crate::{
    graph::{Graph, RepackErrorFlags},
    serialize::Serializer,
};
//TODO: add more functionality, serialize output etc.
pub(crate) fn resolve_overflows(s: &Serializer) -> Result<(), RepackErrorFlags> {
    let mut graph = Graph::from_serializer(s)?;
    graph.sort_shortest_distance()
}
