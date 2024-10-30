//! serializer
//! ported from Harfbuzz Serializer: <https://github.com/harfbuzz/harfbuzz/blob/5e32b5ca8fe430132b87c0eee6a1c056d37c35eb/src/hb-serialize.hh>

use fnv::FnvHashMap;
use write_fonts::types::Scalar;

#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)]
pub(crate) struct SerializeErrorFlags(u16);

#[allow(dead_code)]
impl SerializeErrorFlags {
    pub const SERIALIZE_ERROR_NONE: Self = Self(0x0000);
    pub const SERIALIZE_ERROR_OTHER: Self = Self(0x0001);
    pub const SERIALIZE_ERROR_OFFSET_OVERFLOW: Self = Self(0x0002);
    pub const SERIALIZE_ERROR_OUT_OF_ROOM: Self = Self(0x0004);
    pub const SERIALIZE_ERROR_INT_OVERFLOW: Self = Self(0x0008);
    pub const SERIALIZE_ERROR_ARRAY_OVERFLOW: Self = Self(0x0010);
}

impl Default for SerializeErrorFlags {
    fn default() -> Self {
        Self::SERIALIZE_ERROR_NONE
    }
}

impl std::ops::BitOrAssign for SerializeErrorFlags {
    /// Adds the set of flags.
    #[inline]
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl std::ops::Not for SerializeErrorFlags {
    type Output = bool;
    #[inline]
    fn not(self) -> bool {
        self == SerializeErrorFlags::SERIALIZE_ERROR_NONE
    }
}

#[allow(dead_code)]
#[derive(Default)]
// Offset relative to the current object head (default)/tail or
// Absolute: from the start of the serialize buffer
enum OffsetWhence {
    #[default]
    Head,
    Tail,
    Absolute,
}

type PoolIdx = usize;
// Reference Harfbuzz implementation:
// <https://github.com/harfbuzz/harfbuzz/blob/5e32b5ca8fe430132b87c0eee6a1c056d37c35eb/src/hb-serialize.hh#L69>
#[derive(Default)]
#[allow(dead_code)]
struct Object {
    // head/tail: indices of the output buffer for this object
    head: usize,
    tail: usize,
    // real links are associated with actual offsets
    real_links: Vec<Link>,
    // virtual links not associated with actual offsets,
    // they exist merely to enforce an ordering constraint.
    virtual_links: Vec<Link>,

    // pool index of the object that will be worked on next
    next_obj: Option<PoolIdx>,
}

// LinkWidth:2->offset16, 3->offset24 and 4->offset32
#[allow(dead_code)]
#[derive(Default)]
enum LinkWidth {
    #[default]
    Two,
    Three,
    Four,
}

type ObjIdx = u32;

#[allow(dead_code)]
#[derive(Default)]
struct Link {
    width: LinkWidth,
    is_signed: bool,
    whence: OffsetWhence,
    bias: u32,
    position: u32,
    objidx: Option<ObjIdx>,
}

// reference harfbuzz implementation:
//<https://github.com/harfbuzz/harfbuzz/blob/5e32b5ca8fe430132b87c0eee6a1c056d37c35eb/src/hb-serialize.hh#L57>
#[derive(Default)]
#[allow(dead_code)]
pub(crate) struct Serializer {
    start: usize,
    end: usize,
    head: usize,
    tail: usize,
    // TODO: zerocopy, debug_depth
    errors: SerializeErrorFlags,

    data: Vec<u8>,
    object_pool: ObjectPool,
    // index for current Object in the object_pool
    current: Option<PoolIdx>,

    packed: Vec<Option<ObjIdx>>,
    packed_map: FnvHashMap<usize, ObjIdx>,
}

#[allow(dead_code)]
impl Serializer {
    pub(crate) fn new(size: u32) -> Self {
        let buf_size = size as usize;
        let mut this = Serializer {
            data: vec![0; buf_size],
            end: buf_size,
            tail: buf_size,
            packed: Vec::new(),
            ..Default::default()
        };

        this.packed.push(None);
        this
    }

    // Embed a single Scalar type
    pub(crate) fn embed(&mut self, obj: impl Scalar) -> Result<usize, SerializeErrorFlags> {
        let raw = obj.to_raw();
        let bytes = raw.as_ref();
        let size = bytes.len();

        let ret = self.allocate_size(size)?;
        self.data[ret..ret + size].copy_from_slice(bytes);
        Ok(ret)
    }

    // Allocate size
    pub(crate) fn allocate_size(&mut self, size: usize) -> Result<usize, SerializeErrorFlags> {
        if self.in_error() {
            return Err(self.errors);
        }

        if size > u32::MAX as usize || self.tail - self.head < size {
            return Err(self.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OUT_OF_ROOM));
        }

        //TODO: add support for clear?
        let ret = self.head;
        self.head += size;
        Ok(ret)
    }

    pub(crate) fn successful(&self) -> bool {
        !self.errors
    }

    pub(crate) fn in_error(&self) -> bool {
        !!self.errors
    }

    pub(crate) fn set_err(&mut self, error_type: SerializeErrorFlags) -> SerializeErrorFlags {
        self.errors |= error_type;
        self.errors
    }

    pub(crate) fn push(&mut self) -> Result<(), SerializeErrorFlags> {
        if self.in_error() {
            return Err(self.errors);
        }

        let pool_idx = self.object_pool.alloc();
        let Some(obj) = self.object_pool.get_obj(pool_idx) else {
            return Err(self.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER));
        };

        obj.head = self.head;
        obj.tail = self.tail;
        obj.next_obj = self.current;
        self.current = Some(pool_idx);

        Ok(())
    }

    pub(crate) fn copy_bytes(mut self) -> Result<Vec<u8>, SerializeErrorFlags> {
        if !self.successful() {
            return Err(self.errors);
        }
        let len = (self.head - self.start) + (self.end - self.tail);
        if len == 0 {
            return Ok(Vec::new());
        }

        self.data.copy_within(self.tail..self.end, self.head);
        self.data.truncate(len);
        Ok(self.data)
    }
}

#[allow(dead_code)]
#[derive(Default)]
struct ObjectWrap {
    pub obj: Object,
    pub next: Option<PoolIdx>,
}

// reference Harfbuzz implementation:
//<https://github.com/harfbuzz/harfbuzz/blob/5e32b5ca8fe430132b87c0eee6a1c056d37c35eb/src/hb-pool.hh>
#[allow(dead_code)]
#[derive(Default)]
struct ObjectPool {
    chunks: Vec<ObjectWrap>,
    next: Option<PoolIdx>,
}

#[allow(dead_code)]
impl ObjectPool {
    const ALLOC_CHUNKS_LEN: usize = 64;
    pub fn alloc(&mut self) -> PoolIdx {
        let len = self.chunks.len();
        if self.next.is_none() {
            let new_len = len + Self::ALLOC_CHUNKS_LEN;
            self.chunks.resize_with(new_len, Default::default);

            for (idx, obj) in self
                .chunks
                .get_mut(len..new_len - 1)
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                obj.next = Some(len + idx + 1);
            }

            self.chunks.last_mut().unwrap().next = None;
            self.next = Some(len);
        }

        let ret = self.next;
        self.next = self.chunks[self.next.unwrap()].next;

        ret.unwrap()
    }

    pub fn release(&mut self, pool_idx: PoolIdx) {
        let Some(obj) = self.chunks.get_mut(pool_idx) else {
            return;
        };

        obj.next = self.next;
        self.next = Some(pool_idx);
    }

    pub fn get_obj(&mut self, pool_idx: PoolIdx) -> Option<&mut Object> {
        self.chunks.get_mut(pool_idx).map(|o| &mut o.obj)
    }
}

#[cfg(test)]
mod test {
    use write_fonts::types::{Offset16, UfWord, Uint24};

    use super::*;

    // test Serializer::embed() works for different Scalar types
    #[test]
    fn test_serializer_embed() {
        let mut s = Serializer::new(2);
        let gid = 1_u32;
        //fail when out of room
        assert_eq!(
            s.embed(gid),
            Err(SerializeErrorFlags::SERIALIZE_ERROR_OUT_OF_ROOM)
        );

        let mut s = Serializer::new(16384);
        assert_eq!(s.embed(gid), Ok(0));

        //check that head is advancing accordingly
        let offset = Offset16::new(20);
        assert_eq!(s.embed(offset), Ok(4));

        let n = Uint24::new(30);
        assert_eq!(s.embed(n), Ok(6));

        let w = UfWord::new(40);
        assert_eq!(s.embed(w), Ok(9));
        assert_eq!(s.head, 11);
        assert_eq!(s.start, 0);
        assert_eq!(s.tail, 16384);
        assert_eq!(s.end, 16384);

        let out = s.copy_bytes().unwrap();
        assert_eq!(out, [0, 0, 0, 1, 0, 20, 0, 0, 30, 0, 40]);
    }

    #[test]
    fn test_push() {
        let mut s = Serializer::new(2);
        assert_eq!(s.push(), Ok(()));
        assert_eq!(
            s.embed(1_u32),
            Err(SerializeErrorFlags::SERIALIZE_ERROR_OUT_OF_ROOM)
        );
        assert_eq!(
            s.push(),
            Err(SerializeErrorFlags::SERIALIZE_ERROR_OUT_OF_ROOM)
        );
    }

    #[test]
    fn test_object_pool() {
        let mut p = ObjectPool::default();

        let i = p.alloc();
        assert_eq!(i, 0);
        let i = p.alloc();
        assert_eq!(i, 1);
        let i = p.alloc();
        assert_eq!(i, 2);
        let i = p.alloc();
        assert_eq!(i, 3);

        p.release(1);
        let i = p.alloc();
        assert_eq!(i, 1);

        p.release(3);
        let i = p.alloc();
        assert_eq!(i, 3);

        let i = p.alloc();
        assert_eq!(i, 4);

        let o = p.get_obj(0).unwrap();
        assert_eq!(o.head, 0);
        assert_eq!(o.tail, 0);
        assert!(o.real_links.is_empty());
        assert!(o.virtual_links.is_empty());
        assert_eq!(o.next_obj, None);

        //test alloc more than 64 chunks, which would grow the internal vector
        for i in 0..64 {
            let idx = p.alloc();
            assert_eq!(idx, 5 + i);
        }
    }
}
