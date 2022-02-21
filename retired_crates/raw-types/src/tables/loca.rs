use crate::*;

pub struct Loca<'a> {
    table: LocaFmt<'a>,
}

enum LocaFmt<'a> {
    Short(&'a [Offset16]),
    Long(&'a [Offset32]),
}

impl<'a> Loca<'a> {
    pub fn new(data: &'a [u8], is_32_bit: bool) -> Option<Self> {
        //let len = data.len();
        let table = if is_32_bit {
            let slice = zerocopy::LayoutVerified::new_slice_unaligned(data)?;
            LocaFmt::Long(slice.into_slice())
        } else {
            let slice = zerocopy::LayoutVerified::new_slice_unaligned(data)?;
            LocaFmt::Short(slice.into_slice())
        };
        Some(Loca { table })
    }

    pub fn get(&self, idx: usize) -> Option<Offset32> {
        match &self.table {
            LocaFmt::Short(array) => array
                .get(idx)
                .map(|off| Offset32::new(off.get() as u32 * 2)),
            LocaFmt::Long(array) => array.get(idx).copied(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = Offset32> + '_ {
        let mut idx = 0;
        std::iter::from_fn(move || {
            let result = self.get(idx);
            idx += 1;
            result
        })
    }
}
