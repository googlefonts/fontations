//! Support for temporary memory allocation, making use of the stack for
//! small sizes.

/// Invokes the callback with a memory buffer of the requested size.
pub(super) fn with_temporary_memory<R>(size: usize, mut f: impl FnMut(&mut [u8]) -> R) -> R {
    if size == 0 {
        return f(&mut []);
    }
    // Wrap in a function and prevent inlining to avoid stack allocation
    // and zeroing if we don't take this code path.
    #[inline(never)]
    fn stack_mem<const STACK_SIZE: usize, R>(size: usize, mut f: impl FnMut(&mut [u8]) -> R) -> R {
        f(&mut [0u8; STACK_SIZE][..size])
    }
    // Use bucketed stack allocations (up to 16k) to prevent excessive zeroing
    // of memory
    if size <= 512 {
        stack_mem::<512, _>(size, f)
    } else if size <= 1024 {
        stack_mem::<1024, _>(size, f)
    } else if size <= 2048 {
        stack_mem::<2048, _>(size, f)
    } else if size <= 4096 {
        stack_mem::<4096, _>(size, f)
    } else if size <= 8192 {
        stack_mem::<8192, _>(size, f)
    } else if size <= 16384 {
        stack_mem::<16384, _>(size, f)
    } else {
        f(&mut vec![0u8; size])
    }
}

/// Allocates a mutable slice of `T` of the given length from the specified
/// buffer.
///
/// Returns the allocated slice and the remainder of the buffer.
pub(super) fn alloc_slice<T>(buf: &mut [u8], len: usize) -> Option<(&mut [T], &mut [u8])>
where
    T: bytemuck::AnyBitPattern + bytemuck::NoUninit,
{
    if len == 0 {
        return Some((Default::default(), buf));
    }
    // 1) Ensure we slice the buffer at a position that is properly aligned
    // for T.
    let base_ptr = buf.as_ptr() as usize;
    let aligned_ptr = align_up(base_ptr, core::mem::align_of::<T>());
    let aligned_offset = aligned_ptr - base_ptr;
    let buf = buf.get_mut(aligned_offset..)?;
    // 2) Ensure we have enough space in the buffer to allocate our slice.
    let len_in_bytes = len * size_of::<T>();
    if len_in_bytes > buf.len() {
        return None;
    }
    let (slice_buf, rest) = buf.split_at_mut(len_in_bytes);
    // Bytemuck handles all safety guarantees here.
    let slice = bytemuck::try_cast_slice_mut(slice_buf).ok()?;
    Some((slice, rest))
}

fn align_up(len: usize, alignment: usize) -> usize {
    len + (len.wrapping_neg() & (alignment - 1))
}
