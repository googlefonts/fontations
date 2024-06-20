//! Writes individual bits to a vector of bytes.

pub(crate) struct OutputBitStream<const BF: u32> {
    data: Vec<u8>,
    sub_index: u32,
}

impl<const BF: u32> OutputBitStream<BF> {
    pub(crate) fn new(height: u8) -> OutputBitStream<BF> {
        let mut out = OutputBitStream {
            data: vec![],
            sub_index: 0,
        };
        if height >= 32 {
            panic!("Height value exceeds 5 bits.");
        }
        out.write_header(height);
        out
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }

    /// Writes a single node worth of bits to the stream.
    ///
    /// branch_factor controls the node size.
    pub fn write_node(&mut self, bits: u32) {
        for byte_index in 0..bytes_per_node(BF) {
            if nodes_per_byte(BF) == 1 || self.sub_index == 0 {
                self.data.push(0);
            }

            let bits = (bits >> (byte_index * 8)) & byte_mask(BF);
            let bits = (bits << (self.sub_index * BF)) as u8;
            *self.data.last_mut().unwrap() |= bits;

            if nodes_per_byte(BF) > 1 {
                self.sub_index = (self.sub_index + 1) % nodes_per_byte(BF);
            }
        }
    }

    /// Writes the header byte for a sparse bit set.
    ///
    /// See: https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding
    fn write_header(&mut self, height: u8) {
        let byte = (height & 0b00011111) << 2;
        let byte = byte
            | match BF {
                2 => 0b00,
                4 => 0b01,
                8 => 0b10,
                32 => 0b11,
                _ => panic!("Invalid branch factor"),
            };
        self.data.push(byte);
    }
}

fn nodes_per_byte(branch_factor: u32) -> u32 {
    match branch_factor {
        2 => 4,
        4 => 2,
        8 => 1,
        32 => 1,
        _ => panic!("Invalid branch factor"),
    }
}

fn bytes_per_node(branch_factor: u32) -> u32 {
    match branch_factor {
        2 => 1,
        4 => 1,
        8 => 1,
        32 => 4,
        _ => panic!("Invalid branch factor"),
    }
}

fn byte_mask(branch_factor: u32) -> u32 {
    match branch_factor {
        2 => 0b00000011,
        4 => 0b00001111,
        8 => 0b11111111,
        32 => 0b11111111,
        _ => panic!("Invalid branch factor"),
    }
}

#[cfg(test)]
#[allow(clippy::unusual_byte_groupings)]
mod test {
    use super::*;

    #[test]
    fn init() {
        let os = OutputBitStream::<2>::new(13);
        assert_eq!(os.into_bytes(), vec![0b0_01101_00]);

        let os = OutputBitStream::<4>::new(23);
        assert_eq!(os.into_bytes(), vec![0b0_10111_01]);

        let os = OutputBitStream::<8>::new(1);
        assert_eq!(os.into_bytes(), vec![0b0_00001_10]);

        let os = OutputBitStream::<32>::new(31);
        assert_eq!(os.into_bytes(), vec![0b0_11111_11]);
    }

    #[test]
    fn bf2() {
        let mut os = OutputBitStream::<2>::new(13);

        os.write_node(0b10);
        os.write_node(0b00);
        os.write_node(0b11);
        os.write_node(0b01);

        os.write_node(0b01);
        os.write_node(0b11);

        assert_eq!(
            os.into_bytes(),
            vec![0b0_01101_00, 0b01_11_00_10, 0b00_00_11_01,]
        );
    }

    #[test]
    fn bf4() {
        let mut os = OutputBitStream::<4>::new(23);

        os.write_node(0b0010);
        os.write_node(0b0111);

        os.write_node(0b1101);

        assert_eq!(
            os.into_bytes(),
            vec![0b0_10111_01, 0b0111_0010, 0b0000_1101,]
        );
    }

    #[test]
    fn bf8() {
        let mut os = OutputBitStream::<8>::new(1);

        os.write_node(0b01110010);
        os.write_node(0b00001101);

        assert_eq!(os.into_bytes(), vec![0b0_00001_10, 0b01110010, 0b00001101,]);
    }

    #[test]
    fn bf32() {
        let mut os = OutputBitStream::<32>::new(31);

        os.write_node(0b10000000_00000000_00001101_01110010);

        assert_eq!(
            os.into_bytes(),
            vec![0b0_11111_11, 0b01110010, 0b00001101, 0b00000000, 0b10000000]
        );
    }

    #[test]
    fn truncating() {
        let mut os = OutputBitStream::<4>::new(23);

        os.write_node(0b11110010);

        assert_eq!(os.into_bytes(), vec![0b0_10111_01, 0b0000_0010]);
    }
}
