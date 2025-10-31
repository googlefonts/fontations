//! Implement repacking algorithm to resolve offset overflows
//! ref:<https://github.com/harfbuzz/harfbuzz/blob/9cfb0e6786ecceabaec7a26fd74b1ddb1209f74d/src/hb-repacker.hh#L454>

use crate::{
    graph::{Graph, RepackErrorFlags},
    serialize::Serializer,
};
use write_fonts::{
    read::{
        tables::{gpos::Gpos, gsub::Gsub},
        TopLevelTable,
    },
    types::Tag,
};
//TODO: add more functionality, serialize output etc.
pub(crate) fn resolve_overflows(s: &Serializer, tag: Tag) -> Result<Vec<u8>, RepackErrorFlags> {
    let mut graph = Graph::from_serializer(s)?;

    graph.is_fully_connected()?;
    resolve_graph_overflows(&mut graph, tag)?;

    graph
        .serialize()
        .map_err(|_| RepackErrorFlags::REPACK_ERROR_SERIALIZE)
}

pub(crate) fn resolve_graph_overflows(graph: &mut Graph, tag: Tag) -> Result<(), RepackErrorFlags> {
    graph.sort_shortest_distance()?;
    if !graph.has_overflows() {
        return Ok(());
    }

    if tag == Gsub::TAG || tag == Gpos::TAG {
        if graph.assign_spaces()? {
            graph.sort_shortest_distance()?;
        } else {
            graph.sort_shortest_distance_if_needed()?;
        }
    }

    //TODO: add more overflow resolution
    Err(RepackErrorFlags::REPACK_ERROR_NO_RESOLUTION)
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use crate::graph::test::{populate_serializer_spaces, populate_serializer_with_overflow};

    fn run_resolve_overflow_test(
        overflowing: &Serializer,
        expected: &Serializer,
        check_binary_equivalence: bool,
    ) {
        let mut graph = Graph::from_serializer(overflowing).unwrap();
        let mut expected_graph = Graph::from_serializer(expected).unwrap();

        if expected_graph.has_overflows() {
            if check_binary_equivalence {
                println!("when binary equivalence checking is enabled, the expected graph cannot overflow.");
                assert!(!check_binary_equivalence);
            }
            expected_graph.assign_spaces().unwrap();
            expected_graph.sort_shortest_distance().unwrap();
        }
        // Check that overflow resolution succeeds
        assert!(graph.has_overflows());
        resolve_graph_overflows(&mut graph, Tag::new(b"GSUB")).unwrap();

        // Check the graphs can be serialized.
        let out1 = graph.serialize().unwrap();
        assert!(!out1.is_empty());
        let out2 = expected_graph.serialize().unwrap();
        assert!(!out2.is_empty());

        if check_binary_equivalence {
            assert_eq!(out1, out2);
        }

        // Check the graphs are equivalent
        graph.normalize();
        expected_graph.normalize();
        assert_eq!(graph, expected_graph);
    }

    #[test]
    fn test_resolve_overflows_via_sort() {
        let buf_size = 160000;
        let mut s = Serializer::new(buf_size);
        populate_serializer_with_overflow(&mut s);
        let out = resolve_overflows(&s, Tag::from_u32(0)).unwrap();
        assert_eq!(out.len(), 80000 + 3 + 3 * 2);
    }

    #[test]
    fn test_resolve_overflows_via_space_assignment() {
        let buf_size = 160000;
        let mut c = Serializer::new(buf_size);
        populate_serializer_spaces(&mut c, true);

        let mut e = Serializer::new(buf_size);
        populate_serializer_spaces(&mut e, false);

        run_resolve_overflow_test(&c, &e, false);
    }
}
