//! Reads individual bits from a array of bytes.

pub(crate) struct InputBitStream<'a> {
    data: &'a [u8],
    byte_index: usize,
    bit_index: u8,
}

impl<'a> InputBitStream<'a> {
    pub fn from(data: &'a [u8]) -> InputBitStream {
        InputBitStream {
            data,
            byte_index: 0,
            bit_index: 0,
        }
    }

    /// Reads the next two bits and returns the corresponding branch factor.
    ///
    /// See: https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding
    /// Returns None if the stream does not have enough remaining bits.
    pub fn read_branch_factor(&mut self) -> Option<u8> {
        let bit_0 = self.read_bit()?;
        let bit_1 = self.read_bit()?;

        match (bit_1, bit_0) {
            (false, false) => Some(2),
            (false, true) => Some(4),
            (true, false) => Some(8),
            (true, true) => Some(32),
        }
    }

    /// Reads the next 5 bits and returns the corresponding height value.
    ///
    /// See: https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding
    /// Returns None if the stream does not have enough remaining bits.
    pub fn read_height(&mut self) -> Option<u8> {
        let mut val = 0u8;
        for index in 0..5 {
            if self.read_bit()? {
                val |= 1u8 << index;
            }
        }

        Some(val)
    }

    /// Returns the value of the next bit, or None if there are no more bits left.
    ///
    /// Bits are read from least signifcant to most in the current byte.
    pub fn read_bit(&mut self) -> Option<bool> {
        let byte = self.data.get(self.byte_index)?;
        let mask = 1u8 << self.bit_index;
        let bit_value = byte & mask != 0;

        self.move_to_next();

        Some(bit_value)
    }

    /// Skips the next bit in the stream.
    pub fn skip_bit(&mut self) {
        self.move_to_next();
    }

    fn move_to_next(&mut self) {
        self.bit_index = (self.bit_index + 1) % 8;
        if self.bit_index == 0 {
            self.byte_index += 1;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn read_bit() {
        let data = [0b00110010u8, 0b00000010u8];
        let mut stream = InputBitStream::from(&data);

        for bit in [0, 1, 0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0] {
            assert_eq!(stream.read_bit(), Some(bit != 0));
        }

        assert_eq!(stream.read_bit(), None);
    }

    #[test]
    fn read_branch_factor() {
        let data = [0b11100100u8];
        let mut stream = InputBitStream::from(&data);

        assert_eq!(Some(2), stream.read_branch_factor());
        assert_eq!(Some(4), stream.read_branch_factor());
        assert_eq!(Some(8), stream.read_branch_factor());
        assert_eq!(Some(32), stream.read_branch_factor());
        assert_eq!(None, stream.read_branch_factor());
    }

    #[test]
    #[allow(clippy::unusual_byte_groupings)]
    fn read_height() {
        let data = [0b0_00011_10u8, 0b0000_0001u8];
        let mut stream = InputBitStream::from(&data);

        stream.skip_bit();
        stream.skip_bit();

        assert_eq!(Some(3), stream.read_height());
        assert_eq!(Some(2), stream.read_height());
        assert_eq!(None, stream.read_height());
    }

    #[test]
    fn skip() {
        let data = [0b00110010u8];
        let mut stream = InputBitStream::from(&data);

        stream.skip_bit();
        assert_eq!(stream.read_bit(), Some(true));
        assert_eq!(stream.read_bit(), Some(false));
        stream.skip_bit();
        stream.skip_bit();
        stream.skip_bit();
        stream.skip_bit();
        assert_eq!(stream.read_bit(), Some(false));
        assert_eq!(stream.read_bit(), None);

        // Skipping after the end has been reached.
        stream.skip_bit();
        stream.skip_bit();
        assert_eq!(stream.read_bit(), None);
    }
}
