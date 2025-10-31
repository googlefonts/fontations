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
pub(crate) fn resolve_overflows(
    s: &Serializer,
    tag: Tag,
    max_round: u8,
) -> Result<Vec<u8>, RepackErrorFlags> {
    let mut graph = Graph::from_serializer(s)?;

    graph.is_fully_connected()?;
    resolve_graph_overflows(&mut graph, tag, max_round)?;

    graph
        .serialize()
        .map_err(|_| RepackErrorFlags::REPACK_ERROR_SERIALIZE)
}

pub(crate) fn resolve_graph_overflows(
    graph: &mut Graph,
    tag: Tag,
    max_round: u8,
) -> Result<(), RepackErrorFlags> {
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

    let mut round = 0;
    let mut overflows = graph.overflows();
    while !overflows.is_empty() && round < max_round {
        if !graph.try_isolating_subgraphs(&overflows)? {
            round += 1;
            if !graph.process_overflows(&overflows)? {
                break;
            }
        }

        graph.sort_shortest_distance()?;
        let _ = std::mem::replace(&mut overflows, graph.overflows());
    }

    //TODO: add more overflow resolution
    if overflows.is_empty() {
        Ok(())
    } else {
        Err(RepackErrorFlags::REPACK_ERROR_NO_RESOLUTION)
    }
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use crate::graph::test::{
        add_24_offset, add_object, add_offset, add_virtual_offset, add_wide_offset,
        populate_serializer_with_dedup_overflow, populate_serializer_with_overflow, start_object,
    };

    fn populate_serializer_spaces(s: &mut Serializer, with_overflow: bool) {
        let large_string = [b'a'; 70000];
        s.start_serialize().unwrap();

        let obj_i = if with_overflow {
            add_object(s, b"i", 1)
        } else {
            0
        };

        // space 2
        let obj_h = add_object(s, b"h", 1);
        start_object(s, &large_string, 30000);
        add_offset(s, obj_h);

        let obj_e = s.pop_pack(false).unwrap();
        start_object(s, b"b", 1);
        add_offset(s, obj_e);
        let obj_b = s.pop_pack(false).unwrap();

        // space 1
        let obj_i = if !with_overflow {
            add_object(s, b"i", 1)
        } else {
            obj_i
        };

        start_object(s, &large_string, 30000);
        add_offset(s, obj_i);
        let obj_g = s.pop_pack(false).unwrap();

        start_object(s, &large_string, 30000);
        add_offset(s, obj_i);
        let obj_f = s.pop_pack(false).unwrap();

        start_object(s, b"d", 1);
        add_offset(s, obj_g);
        let obj_d = s.pop_pack(false).unwrap();

        start_object(s, b"c", 1);
        add_offset(s, obj_f);
        let obj_c = s.pop_pack(false).unwrap();

        start_object(s, b"a", 1);
        add_wide_offset(s, obj_b);
        add_wide_offset(s, obj_c);
        add_wide_offset(s, obj_d);
        s.pop_pack(false).unwrap();
        s.end_serialize();
    }

    fn populate_serializer_with_isolation_overflow(s: &mut Serializer) {
        let large_bytes = [b'a'; 70000];
        let _ = s.start_serialize();
        let obj_4 = add_object(s, b"4", 1);

        start_object(s, &large_bytes, 60000);
        add_offset(s, obj_4);
        let obj_3 = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 10000);
        add_offset(s, obj_4);
        let obj_2 = s.pop_pack(false).unwrap();

        start_object(s, b"1", 1);
        add_wide_offset(s, obj_3);
        add_offset(s, obj_2);

        s.pop_pack(false).unwrap();
        s.end_serialize();
    }

    fn populate_serializer_with_isolation_overflow_complex(s: &mut Serializer) {
        let large_bytes = [b'a'; 70000];
        let _ = s.start_serialize();
        let obj_f = add_object(s, b"f", 1);

        start_object(s, b"e", 1);
        add_offset(s, obj_f);
        let obj_e = s.pop_pack(false).unwrap();

        start_object(s, b"c", 1);
        add_offset(s, obj_e);
        let obj_c = s.pop_pack(false).unwrap();

        start_object(s, b"d", 1);
        add_offset(s, obj_e);
        let obj_d = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 60000);
        add_offset(s, obj_d);
        let obj_h = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 60000);
        add_offset(s, obj_c);
        add_offset(s, obj_h);
        let obj_b = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 10000);
        add_offset(s, obj_d);
        let obj_g = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 11000);
        add_offset(s, obj_d);
        let obj_i = s.pop_pack(false).unwrap();

        start_object(s, b"a", 1);
        add_wide_offset(s, obj_b);
        add_offset(s, obj_g);
        add_offset(s, obj_i);
        s.pop_pack(false).unwrap();
        s.end_serialize();
    }

    fn populate_serializer_with_isolation_overflow_complex_expected(s: &mut Serializer) {
        let large_bytes = [b'a'; 70000];
        let _ = s.start_serialize();

        // space 1
        let obj_f_prime = add_object(s, b"f", 1);

        start_object(s, b"e", 1);
        add_offset(s, obj_f_prime);
        let obj_e_prime = s.pop_pack(false).unwrap();

        start_object(s, b"d", 1);
        add_offset(s, obj_e_prime);
        let obj_d_prime = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 60000);
        add_offset(s, obj_d_prime);
        let obj_h = s.pop_pack(false).unwrap();

        start_object(s, b"c", 1);
        add_offset(s, obj_e_prime);
        let obj_c = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 60000);
        add_offset(s, obj_c);
        add_offset(s, obj_h);
        let obj_b = s.pop_pack(false).unwrap();

        // space 0
        let obj_f = add_object(s, b"f", 1);

        start_object(s, b"e", 1);
        add_offset(s, obj_f);
        let obj_e = s.pop_pack(false).unwrap();

        start_object(s, b"d", 1);
        add_offset(s, obj_e);
        let obj_d = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 11000);
        add_offset(s, obj_d);
        let obj_i = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 10000);
        add_offset(s, obj_d);
        let obj_g = s.pop_pack(false).unwrap();

        start_object(s, b"a", 1);
        add_wide_offset(s, obj_b);
        add_offset(s, obj_g);
        add_offset(s, obj_i);
        s.pop_pack(false).unwrap();
        s.end_serialize();
    }

    fn populate_serializer_with_isolation_overflow_spaces(s: &mut Serializer) {
        let large_bytes = [b'a'; 70000];
        let _ = s.start_serialize();
        let obj_d = add_object(s, b"f", 1);
        let obj_e = add_object(s, b"f", 1);

        start_object(s, &large_bytes, 60000);
        add_offset(s, obj_d);
        let obj_b = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 60000);
        add_offset(s, obj_e);
        let obj_c = s.pop_pack(false).unwrap();

        start_object(s, b"a", 1);
        add_wide_offset(s, obj_b);
        add_wide_offset(s, obj_c);

        s.pop_pack(false).unwrap();
        s.end_serialize();
    }

    fn populate_serializer_with_multiple_dedup_overflow(s: &mut Serializer) {
        let large_bytes = [b'a'; 70000];
        let _ = s.start_serialize();
        let leaf = add_object(s, b"def", 3);

        const NUM_MIN_NODES: usize = 20;
        let mut mid_nodes = [0; NUM_MIN_NODES];
        for (i, node_obj_idx) in mid_nodes.iter_mut().enumerate() {
            start_object(s, &large_bytes, 10000 + i);
            add_offset(s, leaf);
            *node_obj_idx = s.pop_pack(false).unwrap();
        }

        start_object(s, b"abc", 3);
        for node_obj_idx in mid_nodes {
            add_wide_offset(s, node_obj_idx);
        }
        s.pop_pack(false).unwrap();

        s.end_serialize();
    }

    fn populate_serializer_with_priority_overflow(s: &mut Serializer) {
        let large_bytes = [b'a'; 50000];
        let _ = s.start_serialize();
        let obj_e = add_object(s, b"e", 1);
        let obj_d = add_object(s, b"d", 1);

        start_object(s, &large_bytes, 50000);
        add_offset(s, obj_e);
        let obj_c = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 20000);
        add_offset(s, obj_d);
        let obj_b = s.pop_pack(false).unwrap();

        start_object(s, b"a", 1);
        add_offset(s, obj_b);
        add_offset(s, obj_c);
        s.pop_pack(false);

        s.end_serialize();
    }

    fn populate_serializer_with_priority_overflow_expected(s: &mut Serializer) {
        let large_bytes = [b'a'; 50000];
        let _ = s.start_serialize();
        let obj_e = add_object(s, b"e", 1);

        start_object(s, &large_bytes, 50000);
        add_offset(s, obj_e);
        let obj_c = s.pop_pack(false).unwrap();

        let obj_d = add_object(s, b"d", 1);

        start_object(s, &large_bytes, 20000);
        add_offset(s, obj_d);
        let obj_b = s.pop_pack(false).unwrap();

        start_object(s, b"a", 1);
        add_offset(s, obj_b);
        add_offset(s, obj_c);
        s.pop_pack(false).unwrap();

        s.end_serialize();
    }

    fn populate_serializer_spaces_16bit_connection(s: &mut Serializer) {
        let large_bytes = [b'a'; 70000];
        let _ = s.start_serialize();
        let obj_g = add_object(s, b"g", 1);
        let obj_h = add_object(s, b"h", 1);

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_g);
        let obj_e = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_h);
        let obj_f = s.pop_pack(false).unwrap();

        start_object(s, b"c", 1);
        add_offset(s, obj_e);
        let obj_c = s.pop_pack(false).unwrap();

        start_object(s, b"d", 1);
        add_offset(s, obj_f);
        let obj_d = s.pop_pack(false).unwrap();

        start_object(s, b"b", 1);
        add_offset(s, obj_e);
        add_offset(s, obj_h);
        let obj_b = s.pop_pack(false).unwrap();

        start_object(s, b"a", 1);
        add_offset(s, obj_b);
        add_wide_offset(s, obj_c);
        add_wide_offset(s, obj_d);

        s.pop_pack(false).unwrap();
        s.end_serialize();
    }

    fn populate_serializer_spaces_16bit_connection_expected(s: &mut Serializer) {
        let large_bytes = [b'a'; 70000];
        let _ = s.start_serialize();
        let obj_g_prime = add_object(s, b"g", 1);

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_g_prime);
        let obj_e_prime = s.pop_pack(false).unwrap();

        start_object(s, b"c", 1);
        add_offset(s, obj_e_prime);
        let obj_c = s.pop_pack(false).unwrap();

        let obj_h_prime = add_object(s, b"h", 1);

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_h_prime);
        let obj_f = s.pop_pack(false).unwrap();

        start_object(s, b"d", 1);
        add_offset(s, obj_f);
        let obj_d = s.pop_pack(false).unwrap();

        let obj_g = add_object(s, b"g", 1);

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_g);
        let obj_e = s.pop_pack(false).unwrap();

        let obj_h = add_object(s, b"h", 1);

        start_object(s, b"b", 1);
        add_offset(s, obj_e);
        add_offset(s, obj_h);
        let obj_b = s.pop_pack(false).unwrap();

        start_object(s, b"a", 1);
        add_offset(s, obj_b);
        add_wide_offset(s, obj_c);
        add_wide_offset(s, obj_d);

        s.pop_pack(false).unwrap();
        s.end_serialize();
    }

    fn populate_serializer_short_and_wide_subgraph_root(s: &mut Serializer) {
        let large_bytes = [b'a'; 70000];
        let _ = s.start_serialize();
        let obj_e = add_object(s, b"e", 1);

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_e);
        let obj_c = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_c);
        let obj_d = s.pop_pack(false).unwrap();

        start_object(s, b"b", 1);
        add_offset(s, obj_c);
        add_offset(s, obj_e);
        let obj_b = s.pop_pack(false).unwrap();

        start_object(s, b"a", 1);
        add_offset(s, obj_b);
        add_wide_offset(s, obj_c);
        add_wide_offset(s, obj_d);

        s.pop_pack(false).unwrap();
        s.end_serialize();
    }

    fn populate_serializer_short_and_wide_subgraph_root_expected(s: &mut Serializer) {
        let large_bytes = [b'a'; 70000];
        let _ = s.start_serialize();
        let obj_e_prime = add_object(s, b"e", 1);

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_e_prime);
        let obj_c_prime = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_c_prime);
        let obj_d = s.pop_pack(false).unwrap();

        let obj_e = add_object(s, b"e", 1);

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_e);
        let obj_c = s.pop_pack(false).unwrap();

        start_object(s, b"b", 1);
        add_offset(s, obj_c);
        add_offset(s, obj_e);
        let obj_b = s.pop_pack(false).unwrap();

        start_object(s, b"a", 1);
        add_offset(s, obj_b);
        add_wide_offset(s, obj_c_prime);
        add_wide_offset(s, obj_d);

        s.pop_pack(false).unwrap();
        s.end_serialize();
    }

    fn populate_serializer_with_split_spaces(s: &mut Serializer) {
        let large_bytes = [b'a'; 70000];
        let _ = s.start_serialize();
        let obj_f = add_object(s, b"f", 1);

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_f);
        let obj_d = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_f);
        let obj_e = s.pop_pack(false).unwrap();

        start_object(s, b"b", 1);
        add_offset(s, obj_d);
        let obj_b = s.pop_pack(false).unwrap();

        start_object(s, b"c", 1);
        add_offset(s, obj_e);
        let obj_c = s.pop_pack(false).unwrap();

        start_object(s, b"a", 1);
        add_wide_offset(s, obj_b);
        add_wide_offset(s, obj_c);

        s.pop_pack(false).unwrap();
        s.end_serialize();
    }

    fn populate_serializer_with_split_spaces_expected(s: &mut Serializer) {
        let large_bytes = [b'a'; 70000];
        let _ = s.start_serialize();
        let obj_f_prime = add_object(s, b"f", 1);

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_f_prime);
        let obj_d = s.pop_pack(false).unwrap();

        start_object(s, b"b", 1);
        add_offset(s, obj_d);
        let obj_b = s.pop_pack(false).unwrap();

        let obj_f = add_object(s, b"f", 1);

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_f);
        let obj_e = s.pop_pack(false).unwrap();

        start_object(s, b"c", 1);
        add_offset(s, obj_e);
        let obj_c = s.pop_pack(false).unwrap();

        start_object(s, b"a", 1);
        add_wide_offset(s, obj_b);
        add_wide_offset(s, obj_c);

        s.pop_pack(false).unwrap();
        s.end_serialize();
    }

    fn populate_serializer_with_split_spaces_2(s: &mut Serializer) {
        let large_bytes = [b'a'; 70000];
        let _ = s.start_serialize();
        let obj_f = add_object(s, b"f", 1);

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_f);
        let obj_d = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_f);
        let obj_e = s.pop_pack(false).unwrap();

        start_object(s, b"b", 1);
        add_offset(s, obj_d);
        let obj_b = s.pop_pack(false).unwrap();

        start_object(s, b"c", 1);
        add_offset(s, obj_e);
        let obj_c = s.pop_pack(false).unwrap();

        start_object(s, b"a", 1);
        add_offset(s, obj_b);
        add_wide_offset(s, obj_b);
        add_wide_offset(s, obj_c);

        s.pop_pack(false).unwrap();
        s.end_serialize();
    }

    fn populate_serializer_with_split_spaces_expected_2(s: &mut Serializer) {
        let large_bytes = [b'a'; 70000];
        let _ = s.start_serialize();

        // space 2
        let obj_f_double_prime = add_object(s, b"f", 1);

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_f_double_prime);
        let obj_d_prime = s.pop_pack(false).unwrap();

        start_object(s, b"b", 1);
        add_offset(s, obj_d_prime);
        let obj_b_prime = s.pop_pack(false).unwrap();

        // space 1
        let obj_f_prime = add_object(s, b"f", 1);

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_f_prime);
        let obj_e = s.pop_pack(false).unwrap();

        start_object(s, b"c", 1);
        add_offset(s, obj_e);
        let obj_c = s.pop_pack(false).unwrap();

        // space 0
        let obj_f = add_object(s, b"f", 1);

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_f);
        let obj_d = s.pop_pack(false).unwrap();

        start_object(s, b"b", 1);
        add_offset(s, obj_d);
        let obj_b = s.pop_pack(false).unwrap();

        // Root
        start_object(s, b"a", 1);
        add_offset(s, obj_b);
        add_wide_offset(s, obj_b_prime);
        add_wide_offset(s, obj_c);

        s.pop_pack(false).unwrap();
        s.end_serialize();
    }

    fn populate_serializer_with_24_and_32_bit_offsets(s: &mut Serializer) {
        let large_bytes = [b'a'; 70000];
        let _ = s.start_serialize();

        let obj_f = add_object(s, b"f", 1);
        let obj_g = add_object(s, b"g", 1);
        let obj_j = add_object(s, b"j", 1);
        let obj_k = add_object(s, b"k", 1);

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_f);
        let obj_c = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_g);
        let obj_d = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_j);
        let obj_h = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 40000);
        add_offset(s, obj_k);
        let obj_i = s.pop_pack(false).unwrap();

        start_object(s, b"e", 1);
        add_wide_offset(s, obj_h);
        add_wide_offset(s, obj_i);
        let obj_e = s.pop_pack(false).unwrap();

        start_object(s, b"b", 1);
        add_24_offset(s, obj_c);
        add_24_offset(s, obj_d);
        add_24_offset(s, obj_e);
        let obj_b = s.pop_pack(false).unwrap();

        start_object(s, b"a", 1);
        add_24_offset(s, obj_b);

        s.pop_pack(false).unwrap();
        s.end_serialize();
    }

    fn populate_serializer_virtual_link(s: &mut Serializer) {
        let _ = s.start_serialize();
        let obj_d = add_object(s, b"d", 1);

        start_object(s, b"b", 1);
        add_offset(s, obj_d);
        let obj_b = s.pop_pack(false).unwrap();

        start_object(s, b"e", 1);
        assert!(add_virtual_offset(s, obj_b));
        let obj_e = s.pop_pack(false).unwrap();

        start_object(s, b"c", 1);
        add_offset(s, obj_e);
        let obj_c = s.pop_pack(false).unwrap();

        start_object(s, b"a", 1);
        add_offset(s, obj_b);
        add_offset(s, obj_c);
        s.pop_pack(false).unwrap();

        s.end_serialize();
    }

    fn run_resolve_overflow_test(
        overflowing: &Serializer,
        expected: &Serializer,
        num_iterations: u8,
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
        resolve_graph_overflows(&mut graph, Tag::new(b"GSUB"), num_iterations).unwrap();

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
        let out = resolve_overflows(&s, Tag::from_u32(0), 32).unwrap();
        assert_eq!(out.len(), 80000 + 3 + 3 * 2);
    }

    #[test]
    fn test_resolve_overflows_via_duplication() {
        let buf_size = 160000;
        let mut s = Serializer::new(buf_size);
        populate_serializer_with_dedup_overflow(&mut s);
        let out = resolve_overflows(&s, Tag::from_u32(0), 32).unwrap();
        assert_eq!(out.len(), 10000 + 2 * 2 + 60000 + 2 + 3 * 2);
    }

    #[test]
    fn test_resolve_overflows_via_multiple_duplication() {
        let buf_size = 300000;
        let mut s = Serializer::new(buf_size);
        populate_serializer_with_multiple_dedup_overflow(&mut s);
        let out = resolve_overflows(&s, Tag::from_u32(0), 5).unwrap();
        assert!(!out.is_empty());
    }

    #[test]
    fn test_resolve_overflows_via_space_assignment() {
        let buf_size = 160000;
        let mut c = Serializer::new(buf_size);
        populate_serializer_spaces(&mut c, true);

        let mut e = Serializer::new(buf_size);
        populate_serializer_spaces(&mut e, false);

        run_resolve_overflow_test(&c, &e, 0, false);
    }

    #[test]
    fn test_resolve_overflows_via_priority() {
        let buf_size = 160000;
        let mut s = Serializer::new(buf_size);
        populate_serializer_with_priority_overflow(&mut s);

        let mut e = Serializer::new(buf_size);
        populate_serializer_with_priority_overflow_expected(&mut e);
        run_resolve_overflow_test(&s, &e, 3, false);
    }

    #[test]
    fn test_resolve_overflows_via_isolation() {
        let buf_size = 160000;
        let mut s = Serializer::new(buf_size);
        populate_serializer_with_isolation_overflow(&mut s);

        assert!(s.offset_overflow());
        let out = resolve_overflows(&s, Tag::new(b"GSUB"), 0).unwrap();
        assert!(!out.is_empty());
        assert_eq!(out.len(), 1 + 10000 + 60000 + 1 + 1 + 4 + 3 * 2);
    }

    #[test]
    fn test_resolve_overflows_via_isolation_with_recursive_duplication() {
        let buf_size = 160000;
        let mut s = Serializer::new(buf_size);
        populate_serializer_with_isolation_overflow_complex(&mut s);

        let mut e = Serializer::new(buf_size);
        populate_serializer_with_isolation_overflow_complex_expected(&mut e);

        run_resolve_overflow_test(&s, &e, 0, false);
    }

    #[test]
    fn test_resolve_overflows_via_isolation_spaces() {
        let buf_size = 160000;
        let mut s = Serializer::new(buf_size);
        populate_serializer_with_isolation_overflow_spaces(&mut s);

        assert!(s.offset_overflow());
        let out = resolve_overflows(&s, Tag::new(b"GSUB"), 0).unwrap();
        assert!(!out.is_empty());

        // objects: 3 + 2 * 60000
        // links: 2 * 4 + 2 *  2
        let expected_length = 3 + 2 * 60000 + 2 * 4 + 2 * 2;
        assert_eq!(out.len(), expected_length);
    }

    #[test]
    fn test_resolve_overflows_via_isolating_16bit_space() {
        let buf_size = 160000;
        let mut s = Serializer::new(buf_size);
        populate_serializer_spaces_16bit_connection(&mut s);

        let mut e = Serializer::new(buf_size);
        populate_serializer_spaces_16bit_connection_expected(&mut e);

        run_resolve_overflow_test(&s, &e, 0, false);
    }

    #[test]
    fn test_resolve_overflows_via_isolating_16bit_space_2() {
        let buf_size = 160000;
        let mut s = Serializer::new(buf_size);
        populate_serializer_short_and_wide_subgraph_root(&mut s);

        let mut e = Serializer::new(buf_size);
        populate_serializer_short_and_wide_subgraph_root_expected(&mut e);

        run_resolve_overflow_test(&s, &e, 0, false);
    }

    #[test]
    fn test_resolve_overflows_via_splitting_spaces() {
        let buf_size = 160000;
        let mut s = Serializer::new(buf_size);
        populate_serializer_with_split_spaces(&mut s);

        let mut e = Serializer::new(buf_size);
        populate_serializer_with_split_spaces_expected(&mut e);

        run_resolve_overflow_test(&s, &e, 1, false);
    }

    #[test]
    fn test_resolve_overflows_via_splitting_spaces_2() {
        let buf_size = 160000;
        let mut s = Serializer::new(buf_size);
        populate_serializer_with_split_spaces_2(&mut s);

        let mut e = Serializer::new(buf_size);
        populate_serializer_with_split_spaces_expected_2(&mut e);

        run_resolve_overflow_test(&s, &e, 1, false);
    }

    #[test]
    fn test_resolve_mixed_overflows_via_isolation_spaces() {
        let buf_size = 200000;
        let mut s = Serializer::new(buf_size);
        populate_serializer_with_24_and_32_bit_offsets(&mut s);

        assert!(s.offset_overflow());
        let out = resolve_overflows(&s, Tag::new(b"GSUB"), 0).unwrap();
        assert!(!out.is_empty());

        // objects: 7 + 4 * 40000
        // links:
        // 32bit: 2 * 4
        // 24bit: 4 * 3
        // 16bit: 4 * 2
        let expected_length = 7 + 4 * 40000 + 2 * 4 + 4 * 3 + 4 * 2;
        assert_eq!(out.len(), expected_length);
    }

    #[test]
    fn test_virtual_link() {
        let buf_size = 100;
        let mut c = Serializer::new(buf_size);
        populate_serializer_virtual_link(&mut c);

        let out = resolve_overflows(&c, Tag::from_u32(0), 32).unwrap();
        assert!(!out.is_empty());
        assert_eq!(out.len(), 5 + 4 * 2);
        assert_eq!(out[0], b'a');
        assert_eq!(out[5], b'c');
        assert_eq!(out[8], b'e');
        assert_eq!(out[9], b'b');
        assert_eq!(out[12], b'd');
    }
}
