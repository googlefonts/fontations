//! Implement repacking algorithm to resolve offset overflows
//! ref:<https://github.com/harfbuzz/harfbuzz/blob/9cfb0e6786ecceabaec7a26fd74b1ddb1209f74d/src/hb-repacker.hh#L454>

use crate::{
    graph::{Graph, RepackErrorFlags},
    serialize::Serializer,
};
//TODO: add more functionality, serialize output etc.
pub(crate) fn resolve_overflows(s: &Serializer) -> Result<Vec<u8>, RepackErrorFlags> {
    let mut graph = Graph::from_serializer(s)?;

    graph.is_fully_connected()?;
    resolve_graph_overflows(&mut graph)?;

    graph
        .serialize()
        .map_err(|_| RepackErrorFlags::REPACK_ERROR_SERIALIZE)
}

pub(crate) fn resolve_graph_overflows(graph: &mut Graph) -> Result<(), RepackErrorFlags> {
    graph.sort_shortest_distance()?;
    if !graph.has_overflows() {
        return Ok(());
    }

    //TODO: add more overflow resolution
    Err(RepackErrorFlags::REPACK_ERROR_NO_RESOLUTION)
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use crate::graph::test::populate_serializer_with_overflow;

    #[test]
    fn test_resolve_overflows_via_sort() {
        let buf_size = 160000;
        let mut s = Serializer::new(buf_size);
        populate_serializer_with_overflow(&mut s);
        let out = resolve_overflows(&s).unwrap();
        assert_eq!(out.len(), 80000 + 3 + 3 * 2);
    }
}
