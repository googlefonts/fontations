//! Provides serialization of IntSet's to a highly compact bitset format as defined in the
//! IFT specification:
//!
//! <https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding>

use std::collections::VecDeque;
use std::error::Error;
use std::fmt;

use crate::input_bit_stream::InputBitStream;
use crate::output_bit_stream::OutputBitStream;
use crate::IntSet;

#[derive(Debug)]
pub struct DecodingError;

impl Error for DecodingError {}

impl fmt::Display for DecodingError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "The input data stream was too short to be a valid sparse bit set."
        )
    }
}

#[derive(Copy, Clone)]
pub(crate) enum BranchFactor {
    Two,
    Four,
    Eight,
    ThirtyTwo,
}

impl IntSet<u32> {
    /// Populate this set with the values obtained from decoding the provided sparse bit set bytes.
    ///
    /// Sparse bit sets are a specialized, compact encoding of bit sets defined in the IFT specification:
    /// <https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding>
    pub fn from_sparse_bit_set(data: &[u8]) -> Result<IntSet<u32>, DecodingError> {
        // This is a direct port of the decoding algorithm from:
        // <https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding>
        let mut bits = InputBitStream::from(data);

        let Some(branch_factor) = bits.read_branch_factor() else {
            return Err(DecodingError);
        };

        let Some(height) = bits.read_height() else {
            return Err(DecodingError);
        };

        let mut out = IntSet::<u32>::empty();
        if height == 0 {
            return Ok(out);
        }

        // Bit 8 of header byte is ignored.
        bits.skip_bit();

        let mut queue = VecDeque::<NextNode>::new(); // TODO(garretrieger): estimate initial capacity?
        queue.push_back(NextNode { start: 0, depth: 1 });

        while let Some(next) = queue.pop_front() {
            let mut has_a_one = false;
            for index in 0..branch_factor as u32 {
                let Some(bit) = bits.read_bit() else {
                    return Err(DecodingError);
                };

                if !bit {
                    continue;
                }

                // TODO(garretrieger): use two while loops (one for non-leaf and one for leaf nodes)
                //                     to avoid having to branch on each iteration.
                has_a_one = true;
                if next.depth == height as u32 {
                    // TODO(garretrieger): optimize insertion speed by using the bulk sorted insert
                    // (rewrite this to be an iterator) or even directly writing groups of bits to the pages.
                    out.insert(next.start + index);
                } else {
                    let exp = height as u32 - next.depth;
                    queue.push_back(NextNode {
                        start: next.start + index * (branch_factor as u32).pow(exp),
                        depth: next.depth + 1,
                    });
                }
            }

            if !has_a_one {
                // all bits were zeroes which is a special command to completely fill in
                // all integers covered by this node.
                let exp = (height as u32) - next.depth + 1;
                out.insert_range(next.start..=next.start + (branch_factor as u32).pow(exp) - 1);
            }
        }

        Ok(out)
    }

    /// Encodeg this set as a sparse bit set byte encoding.
    ///
    /// Sparse bit sets are a specialized, compact encoding of bit sets defined in the IFT specification:
    /// <https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding>
    pub fn to_sparse_bit_set(&self) -> Vec<u8> {
        // TODO(garretrieger): use the heuristic approach from the incxfer
        // implementation to guess the optimal size. Building the set 4 times
        // is costly.
        let mut candidates: Vec<Vec<u8>> = vec![];

        let Some(max_value) = self.last() else {
            return OutputBitStream::new(BranchFactor::Two, 0).into_bytes();
        };

        if BranchFactor::Two.tree_height_for(max_value) < OutputBitStream::MAX_HEIGHT {
            candidates.push(to_sparse_bit_set_with_bf::<2>(self));
        }

        if BranchFactor::Four.tree_height_for(max_value) < OutputBitStream::MAX_HEIGHT {
            candidates.push(to_sparse_bit_set_with_bf::<4>(self));
        }

        if BranchFactor::Eight.tree_height_for(max_value) < OutputBitStream::MAX_HEIGHT {
            candidates.push(to_sparse_bit_set_with_bf::<8>(self));
        }

        if BranchFactor::ThirtyTwo.tree_height_for(max_value) < OutputBitStream::MAX_HEIGHT {
            candidates.push(to_sparse_bit_set_with_bf::<32>(self));
        }

        candidates.into_iter().min_by_key(|f| f.len()).unwrap()
    }
}

fn to_sparse_bit_set_with_bf<const BF: u8>(set: &IntSet<u32>) -> Vec<u8> {
    // TODO(garretrieger): implement detection of filled nodes (ie. zero nodes)
    let branch_factor = BranchFactor::from_val(BF);
    let Some(max_value) = set.last() else {
        return OutputBitStream::new(branch_factor, 0).into_bytes();
    };
    let mut height = branch_factor.tree_height_for(max_value);
    let mut os = OutputBitStream::new(branch_factor, height);
    let mut nodes: Vec<Node> = vec![];

    // We build the nodes that will comprise the bit stream in reverse order
    // from the last value in the last layer up to the first layer. Then
    // when generating the final stream the order is reversed.
    // The reverse order construction is needed since nodes at the lower layer
    // affect the values in the parent layers.
    let mut indices = set.clone();
    while height > 0 {
        indices = create_layer(branch_factor, indices.iter(), &mut nodes);
        height -= 1;
    }

    for node in nodes.iter().rev() {
        os.write_node(node.bits);
    }

    os.into_bytes()
}

/// Compute the nodes for a layer of the sparse bit set.
///
/// Computes the nodes needed for the layer which contains the indices in
/// 'iter'. The new nodes are appeded to 'nodes'. 'iter' must be sorted
/// in ascending order.
///
/// Returns the set of indices for the layer above.
fn create_layer<T: DoubleEndedIterator<Item = u32>>(
    branch_factor: BranchFactor,
    iter: T,
    nodes: &mut Vec<Node>,
) -> IntSet<u32> {
    let mut next_indices = IntSet::<u32>::empty();

    // The nodes array is produced in reverse order and then reversed before final output.
    let mut current_node: Option<Node> = None;
    for v in iter.rev() {
        let parent_index = v / branch_factor.value();
        let prev_parent_index = current_node
            .as_ref()
            .map_or(parent_index, |node| node.parent_index);
        if prev_parent_index != parent_index {
            nodes.push(current_node.take().unwrap());
            next_indices.insert(prev_parent_index);
        }

        let current_node = current_node.get_or_insert(Node {
            bits: 0,
            parent_index,
        });

        current_node.bits |= 0b1 << (v % branch_factor.value());
    }
    if let Some(node) = current_node {
        next_indices.insert(node.parent_index);
        nodes.push(node);
    }

    next_indices
}

struct Node {
    bits: u32,
    parent_index: u32,
}

impl BranchFactor {
    pub(crate) fn value(&self) -> u32 {
        match self {
            BranchFactor::Two => 2,
            BranchFactor::Four => 4,
            BranchFactor::Eight => 8,
            BranchFactor::ThirtyTwo => 32,
        }
    }

    fn tree_height_for(&self, max_value: u32) -> u8 {
        // height H, can represent up to (BF^height) - 1
        let mut height: u32 = 0;
        let mut max_value = max_value;
        loop {
            height += 1;
            max_value >>= self.node_size_log2();
            if max_value == 0 {
                break height as u8;
            }
        }
    }

    fn from_val(val: u8) -> BranchFactor {
        match val {
            2 => BranchFactor::Two,
            4 => BranchFactor::Four,
            8 => BranchFactor::Eight,
            32 => BranchFactor::ThirtyTwo,
            // This should never happen as this is only used internally.
            _ => panic!("Invalid branch factor."),
        }
    }

    fn node_size_log2(&self) -> u32 {
        match self {
            BranchFactor::Two => 1,
            BranchFactor::Four => 2,
            BranchFactor::Eight => 3,
            BranchFactor::ThirtyTwo => 5,
        }
    }
}

struct NextNode {
    start: u32,
    depth: u32,
}

#[cfg(test)]
#[allow(clippy::unusual_byte_groupings)]
mod test {
    use super::*;

    #[test]
    fn spec_example_2() {
        // Test of decoding the example 2 given in the specification.
        // See: <https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding>
        let bytes = [
            0b00001110, 0b00100001, 0b00010001, 0b00000001, 0b00000100, 0b00000010, 0b00001000,
        ];

        let set = IntSet::<u32>::from_sparse_bit_set(&bytes).unwrap();
        let expected: IntSet<u32> = [2, 33, 323].iter().copied().collect();
        assert_eq!(set, expected);
    }

    #[test]
    fn spec_example_3() {
        // Test of decoding the example 3 given in the specification.
        // See: <https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding>
        let bytes = [0b00000000];

        let set = IntSet::<u32>::from_sparse_bit_set(&bytes).unwrap();
        let expected: IntSet<u32> = [].iter().copied().collect();
        assert_eq!(set, expected);
    }

    #[test]
    fn spec_example_4() {
        // Test of decoding the example 4 given in the specification.
        // See: <https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding>
        let bytes = [0b00001101, 0b00000011, 0b00110001];

        let set = IntSet::<u32>::from_sparse_bit_set(&bytes).unwrap();

        let mut expected: IntSet<u32> = IntSet::<u32>::empty();
        expected.insert_range(0..=17);

        assert_eq!(set, expected);
    }

    #[test]
    fn invalid() {
        // Spec example 2 with one byte missing.
        let bytes = [
            0b00001110, 0b00100001, 0b00010001, 0b00000001, 0b00000100, 0b00000010,
        ];
        assert!(IntSet::<u32>::from_sparse_bit_set(&bytes).is_err());
    }

    #[test]
    fn test_tree_height_for() {
        assert_eq!(BranchFactor::Two.tree_height_for(0), 1);
        assert_eq!(BranchFactor::Two.tree_height_for(1), 1);
        assert_eq!(BranchFactor::Two.tree_height_for(2), 2);
        assert_eq!(BranchFactor::Two.tree_height_for(117), 7);

        assert_eq!(BranchFactor::Four.tree_height_for(0), 1);
        assert_eq!(BranchFactor::Four.tree_height_for(3), 1);
        assert_eq!(BranchFactor::Four.tree_height_for(4), 2);
        assert_eq!(BranchFactor::Four.tree_height_for(63), 3);
        assert_eq!(BranchFactor::Four.tree_height_for(64), 4);

        assert_eq!(BranchFactor::Eight.tree_height_for(0), 1);
        assert_eq!(BranchFactor::Eight.tree_height_for(7), 1);
        assert_eq!(BranchFactor::Eight.tree_height_for(8), 2);
        assert_eq!(BranchFactor::Eight.tree_height_for(32767), 5);
        assert_eq!(BranchFactor::Eight.tree_height_for(32768), 6);

        assert_eq!(BranchFactor::ThirtyTwo.tree_height_for(0), 1);
        assert_eq!(BranchFactor::ThirtyTwo.tree_height_for(31), 1);
        assert_eq!(BranchFactor::ThirtyTwo.tree_height_for(32), 2);
        assert_eq!(BranchFactor::ThirtyTwo.tree_height_for(1_048_575), 4);
        assert_eq!(BranchFactor::ThirtyTwo.tree_height_for(1_048_576), 5);
    }

    #[test]
    fn generate_spec_example_2() {
        // Test of reproducing the encoding of example 2 given
        // in the specification. See:
        // <https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding>

        let actual_bytes = to_sparse_bit_set_with_bf::<8>(&[2, 33, 323].iter().copied().collect());
        let expected_bytes = [
            0b00001110, 0b00100001, 0b00010001, 0b00000001, 0b00000100, 0b00000010, 0b00001000,
        ];

        assert_eq!(actual_bytes, expected_bytes);
    }

    #[test]
    fn generate_spec_example_3() {
        // Test of reproducing the encoding of example 3 given
        // in the specification. See:
        // <https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding>

        let actual_bytes = to_sparse_bit_set_with_bf::<2>(&IntSet::<u32>::empty());
        let expected_bytes = [0b00000000];

        assert_eq!(actual_bytes, expected_bytes);
    }

    #[test]
    fn encode_one_level() {
        let actual_bytes = to_sparse_bit_set_with_bf::<8>(&[2, 6].iter().copied().collect());
        let expected_bytes = [0b0_00001_10, 0b01000100];
        assert_eq!(actual_bytes, expected_bytes);
    }

    #[test]
    fn encode_bf32() {
        let actual_bytes = to_sparse_bit_set_with_bf::<32>(&[2, 31, 323].iter().copied().collect());
        let expected_bytes = [
            0b0_00010_11,
            // node 0
            0b00000001,
            0b00000100,
            0b00000000,
            0b00000000,
            // node 1
            0b00000100,
            0b00000000,
            0b00000000,
            0b10000000,
            // node 2
            0b00001000,
            0b00000000,
            0b00000000,
            0b00000000,
        ];

        assert_eq!(actual_bytes, expected_bytes);
    }

    #[test]
    fn round_trip() {
        let s1: IntSet<u32> = [11, 74, 9358].iter().copied().collect();
        let mut s2: IntSet<u32> = s1.clone();
        s2.insert_range(67..=412);

        check_round_trip::<2>(&s1);
        check_round_trip::<4>(&s1);
        check_round_trip::<8>(&s1);
        check_round_trip::<32>(&s1);

        check_round_trip::<2>(&s2);
        check_round_trip::<4>(&s2);
        check_round_trip::<8>(&s2);
        check_round_trip::<32>(&s2);
    }

    fn check_round_trip<const BF: u8>(s: &IntSet<u32>) {
        let bytes = to_sparse_bit_set_with_bf::<BF>(s);
        let s_prime = IntSet::<u32>::from_sparse_bit_set(&bytes).unwrap();
        assert_eq!(*s, s_prime);
    }

    #[test]
    fn find_smallest_bf() {
        let s: IntSet<u32> = [11, 74, 9358].iter().copied().collect();
        let bytes = s.to_sparse_bit_set();
        // BF4
        assert_eq!(vec![0b0_00111_01], bytes[0..1]);

        let s: IntSet<u32> = [
            16, 0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28, 30,
        ]
        .iter()
        .copied()
        .collect();
        let bytes = s.to_sparse_bit_set();
        // BF32
        assert_eq!(vec![0b0_00001_11], bytes[0..1]);
    }

    #[test]
    fn encode_maxu32() {
        let s: IntSet<u32> = [1, u32::MAX].iter().copied().collect();
        let bytes = s.to_sparse_bit_set();
        let s_prime = IntSet::<u32>::from_sparse_bit_set(&bytes);
        assert_eq!(s, s_prime.unwrap());
    }
}
