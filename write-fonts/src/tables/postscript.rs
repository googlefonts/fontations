//! Postscript (CFF/CFF2) table related code.

include!("../../generated/generated_postscript.rs");

impl Index2 {
    /// Construct an `Index2` from a list of byte items.
    pub fn from_items(items: Vec<Vec<u8>>) -> Self {
        Self { data: items }
    }
}

impl FontWrite for Index1 {
    fn write_into(&self, writer: &mut TableWriter) {
        IndexWriter {
            format: IndexFormat::Format1,
            objects: self.data.as_slice(),
        }
        .write_into(writer);
    }
}

impl FontWrite for Index2 {
    fn write_into(&self, writer: &mut TableWriter) {
        IndexWriter {
            format: IndexFormat::Format2,
            objects: self.data.as_slice(),
        }
        .write_into(writer);
    }
}

fn convert_objects_f1(from: &read_fonts::tables::postscript::Index1) -> Vec<Vec<u8>> {
    (0..from.count())
        .map(|i| from.get(i as usize).map(Vec::from).unwrap_or_default())
        .collect()
}

fn convert_objects_f2(from: &read_fonts::tables::postscript::Index2) -> Vec<Vec<u8>> {
    (0..from.count())
        .map(|i| from.get(i as usize).map(Vec::from).unwrap_or_default())
        .collect()
}

enum IndexFormat {
    Format1,
    Format2,
}
struct IndexWriter<'a> {
    format: IndexFormat,
    objects: &'a [Vec<u8>],
}

impl FontWrite for IndexWriter<'_> {
    fn write_into(&self, writer: &mut TableWriter) {
        let count = self.objects.len();
        match self.format {
            IndexFormat::Format1 => (count as u16).write_into(writer),
            IndexFormat::Format2 => (count as u32).write_into(writer),
        }

        // Calculate offsets (1-based per CFF2 spec)
        let mut offset_values = Vec::with_capacity(count);
        let mut current_offset = 1u32;
        // always start with 1
        offset_values.push(current_offset);
        for item in self.objects {
            current_offset += item.len() as u32;
            offset_values.push(current_offset);
        }

        let off_size = (4 - current_offset.leading_zeros() / 8).max(1) as u8;
        off_size.write_into(writer);

        for offset in offset_values.iter().copied() {
            match off_size {
                1 => (offset as u8).write_into(writer),
                2 => (offset as u16).write_into(writer),
                3 => Uint24::new(offset).write_into(writer),
                4 => offset.write_into(writer),
                _ => unreachable!(),
            }
        }

        for data in self.objects {
            data.write_into(writer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index2_from_items() {
        let items = vec![vec![1, 2, 3], vec![4, 5]];
        let index = Index2::from_items(items);
        let bytes = crate::dump_table(&index).unwrap();
        let read_back = Index2::read(bytes.as_slice().into()).unwrap();

        assert_eq!(index, read_back);
    }
}
