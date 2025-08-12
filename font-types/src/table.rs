//! Support for parsing tables with a fixed header.

/// Raw bytes of a table along with a reference to the fixed size header.
#[derive(Copy, Clone)]
pub struct TableDataWithHeader<'a, H> {
    data: &'a [u8],
    _marker: core::marker::PhantomData<&'a H>,
}

impl<'a, H> TableDataWithHeader<'a, H>
where
    H: bytemuck::AnyBitPattern + bytemuck::Zeroable,
{
    #[inline]
    pub fn new(data: &'a [u8]) -> Option<Self> {
        let header_size = core::mem::size_of::<H>();
        let header_data = data.get(..header_size)?;
        // SAFETY: ensure that we can safetly reinterpret the initial bytes of
        // `data` as `H`.
        let _header: &H = bytemuck::try_from_bytes(header_data).ok()?;
        Some(Self {
            data,
            _marker: core::marker::PhantomData,
        })
    }

    /// Returns the fixed header at the beginning of the data.
    #[inline(always)]
    pub const fn header(&self) -> &'a H {
        // SAFETY: The constructor ensured that the data is of sufficient
        // length and our bytemuck cast checked that we meet all other
        // required conditions for this conversion.
        unsafe { &*(self.data.as_ptr() as *const H) }
    }
}

impl<'a, H> TableDataWithHeader<'a, H> {
    /// Returns the slice containing the full table data.
    #[inline(always)]
    pub const fn data(&self) -> &'a [u8] {
        self.data
    }

    /// Returns the size of the table header.
    #[inline(always)]
    pub const fn header_size(&self) -> usize {
        core::mem::size_of::<H>()
    }
}
