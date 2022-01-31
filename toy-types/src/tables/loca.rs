use crate::*;

pub struct Loca<'a> {
    table: LocaFmt<'a>,
}

enum LocaFmt<'a> {
    Short(Array<'a, Offset16>),
    Long(Array<'a, Offset32>),
}

impl<'a> Loca<'a> {
    pub fn new(data: Blob<'a>, is_32_bit: bool) -> Option<Self> {
        let len = data.len();
        let table = if is_32_bit {
            LocaFmt::Long(Array::new(data, 0, len / 4)?)
        } else {
            LocaFmt::Short(Array::new(data, 0, len / 2)?)
        };
        Some(Loca { table })
    }

    pub fn get(&self, idx: usize) -> Option<Offset32> {
        match &self.table {
            LocaFmt::Short(array) => array.get(idx).map(|off| off as Offset32 * 2),
            LocaFmt::Long(array) => array.get(idx),
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
