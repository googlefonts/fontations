//! Provides serialization of IntSet's to a highly compact bitset format as defined in the
//! IFT specification:
//!
//! https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding

use std::collections::VecDeque;

use crate::input_bit_stream::InputBitStream;
use crate::IntSet;
use thiserror::Error;

#[derive(Error, Debug)]
#[error("The input data stream was too short to be a valid sparse bit set.")]
pub struct DecodingError();

pub(crate) fn to_sparse_bit_set(_set: &IntSet<u32>) -> Vec<u8> {
    todo!()
}

struct NextNode {
    start: u32,
    depth: u32,
}

pub(crate) fn from_sparse_bit_set(data: &[u8]) -> Result<IntSet<u32>, DecodingError> {
    // This is a direct port of the decoding algorithm from:
    // https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding
    let mut bits = InputBitStream::from(data);

    let Some(branch_factor) = bits.read_branch_factor() else {
        return Err(DecodingError());
    };

    let Some(height) = bits.read_height() else {
        return Err(DecodingError());
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
                return Err(DecodingError());
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn spec_example_2() {
        // Test of decoding the example 2 given in the specification.
        // See: https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding
        let bytes = [
            0b00001110, 0b00100001, 0b00010001, 0b00000001, 0b00000100, 0b00000010, 0b00001000,
        ];

        let set = from_sparse_bit_set(&bytes).unwrap();
        let expected: IntSet<u32> = [2, 33, 323].iter().copied().collect();
        assert_eq!(set, expected);
    }

    #[test]
    fn spec_example_3() {
        // Test of decoding the example 3 given in the specification.
        // See: https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding
        let bytes = [0b00000000];

        let set = from_sparse_bit_set(&bytes).unwrap();
        let expected: IntSet<u32> = [].iter().copied().collect();
        assert_eq!(set, expected);
    }

    #[test]
    fn spec_example_4() {
        // Test of decoding the example 4 given in the specification.
        // See: https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding
        let bytes = [0b00001101, 0b00000011, 0b00110001];

        let set = from_sparse_bit_set(&bytes).unwrap();

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
        assert!(from_sparse_bit_set(&bytes).is_err());
    }
}
