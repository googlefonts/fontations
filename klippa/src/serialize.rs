//! serializer
//! ported from Harfbuzz Serializer: <https://github.com/harfbuzz/harfbuzz/blob/5e32b5ca8fe430132b87c0eee6a1c056d37c35eb/src/hb-serialize.hh>
use std::{
    hash::{Hash, Hasher},
    mem,
};

use fnv::FnvHasher;
use hashbrown::HashTable;
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
#[derive(Default, Eq, PartialEq, Hash)]
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
#[allow(dead_code)]
#[derive(Default)]
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

impl Object {
    fn reset(&mut self) {
        self.real_links.clear();
        self.virtual_links.clear();
    }
}

// LinkWidth:2->offset16, 3->offset24 and 4->offset32
#[allow(dead_code)]
#[derive(Default, Eq, PartialEq, Hash)]
enum LinkWidth {
    #[default]
    Two,
    Three,
    Four,
}

type ObjIdx = usize;

#[allow(dead_code)]
#[derive(Default, Eq, PartialEq, Hash)]
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

    packed: Vec<Option<PoolIdx>>,
    packed_map: PoolIdxHashTable,
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

    pub(crate) fn only_overflow(&self) -> bool {
        self.errors == SerializeErrorFlags::SERIALIZE_ERROR_OFFSET_OVERFLOW
            || self.errors == SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW
            || self.errors == SerializeErrorFlags::SERIALIZE_ERROR_ARRAY_OVERFLOW
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
        let Some(obj) = self.object_pool.get_obj_mut(pool_idx) else {
            return Err(self.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER));
        };

        obj.head = self.head;
        obj.tail = self.tail;
        obj.next_obj = self.current;
        self.current = Some(pool_idx);

        Ok(())
    }

    pub(crate) fn pop_pack(&mut self, share: bool) -> Option<ObjIdx> {
        self.current?;

        // Allow cleanup when we've error'd out on int overflows which don't compromise the serializer state
        if self.in_error() && !self.only_overflow() {
            return None;
        }

        let pool_idx = self.current.unwrap();
        // code logic is a bit different from Harfbuzz serializer here
        // because I need to move code around to fix mutable borrow/immutable borrow issue
        let obj = self.object_pool.get_obj_mut(pool_idx)?;

        self.current = obj.next_obj;
        obj.tail = self.head;
        obj.next_obj = None;

        if obj.tail < obj.head {
            self.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            return None;
        }

        let len = obj.tail - obj.head;

        // TODO: consider zerocopy
        // Rewind head
        self.head = obj.head;

        if len == 0 {
            if !obj.real_links.is_empty() || !obj.virtual_links.is_empty() {
                self.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            }
            return None;
        }

        self.tail -= len;

        let obj_head = obj.head;
        obj.head = self.tail;
        obj.tail = self.tail + len;

        //TODO: consider zerocopy here
        self.data.copy_within(obj_head..obj_head + len, self.tail);

        let mut hash = 0_u64;
        // introduce flags to avoid mutable borrow and immutable borrow issues
        let mut obj_duplicate = None;
        if share {
            hash = hash_one_pool_idx(pool_idx, &self.data, &self.object_pool);
            if let Some(entry) =
                self.packed_map
                    .get_with_hash(pool_idx, hash, &self.data, &self.object_pool)
            {
                obj_duplicate = Some(*entry);
            };
        }

        if let Some(dup_obj_idxes) = obj_duplicate {
            self.merge_virtual_links(pool_idx, dup_obj_idxes);
            self.object_pool.release(pool_idx);

            // rewind tail because we discarded duplicate obj
            self.tail += len;
            return Some(dup_obj_idxes.1);
        }

        self.packed.push(Some(pool_idx));
        let obj_idx = self.packed.len() - 1;

        if share {
            self.packed_map
                .set_with_hash(pool_idx, hash, obj_idx, &self.data, &self.object_pool);
        }
        Some(obj_idx)
    }

    fn merge_virtual_links(&mut self, from: PoolIdx, to: (PoolIdx, ObjIdx)) {
        let from_obj = self.object_pool.get_obj_mut(from).unwrap();
        if from_obj.virtual_links.is_empty() {
            return;
        }
        let mut from_vec = mem::take(&mut from_obj.virtual_links);

        // hash value will change after merge, so delete existing old entry in the hash table
        let hash_old = hash_one_pool_idx(to.0, &self.data, &self.object_pool);
        self.packed_map
            .del(to.0, hash_old, &self.data, &self.object_pool);

        let to_obj = self.object_pool.get_obj_mut(to.0).unwrap();
        to_obj.virtual_links.append(&mut from_vec);

        let hash = hash_one_pool_idx(to.0, &self.data, &self.object_pool);
        self.packed_map
            .set_with_hash(to.0, hash, to.1, &self.data, &self.object_pool);
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

        let pool_idx = self.next.unwrap();
        self.next = self.chunks[pool_idx].next;

        pool_idx
    }

    pub fn release(&mut self, pool_idx: PoolIdx) {
        let Some(obj_wrap) = self.chunks.get_mut(pool_idx) else {
            return;
        };

        obj_wrap.obj.reset();
        obj_wrap.next = self.next;
        self.next = Some(pool_idx);
    }

    pub fn get_obj_mut(&mut self, pool_idx: PoolIdx) -> Option<&mut Object> {
        self.chunks.get_mut(pool_idx).map(|o| &mut o.obj)
    }

    pub fn get_obj(&self, pool_idx: PoolIdx) -> Option<&Object> {
        self.chunks.get(pool_idx).map(|o| &o.obj)
    }
}

// Hash an Object: Virtual links aren't considered for equality since they don't affect the functionality of the object.
fn hash_one_pool_idx(pool_idx: PoolIdx, data: &[u8], obj_pool: &ObjectPool) -> u64 {
    let mut hasher = FnvHasher::default();
    let Some(obj) = obj_pool.get_obj(pool_idx) else {
        return hasher.finish();
    };
    // hash data bytes
    // Only hash at most 128 bytes for Object. Byte objects differ in their early bytes anyway.
    // reference: <https://github.com/harfbuzz/harfbuzz/blob/622e9c33c39e9c2a6491763841d6d6ad715f6abf/src/hb-serialize.hh#L136>
    let byte_len = 128.min(obj.tail - obj.head);
    let data_bytes = data.get(obj.head..obj.head + byte_len);
    data_bytes.hash(&mut hasher);
    // hash real_links
    obj.real_links.hash(&mut hasher);

    hasher.finish()
}

// Virtual links aren't considered for equality since they don't affect the functionality of the object.
fn cmp_pool_idx(idx_a: PoolIdx, idx_b: PoolIdx, data: &[u8], obj_pool: &ObjectPool) -> bool {
    if idx_a == idx_b {
        return true;
    }

    match (obj_pool.get_obj(idx_a), obj_pool.get_obj(idx_b)) {
        (Some(_), None) => false,
        (None, Some(_)) => false,
        (None, None) => true,
        (Some(obj_a), Some(obj_b)) => {
            data.get(obj_a.head..obj_a.tail) == data.get(obj_b.head..obj_b.tail)
                && obj_a.real_links == obj_b.real_links
        }
    }
}

#[derive(Default)]
struct PoolIdxHashTable {
    hash_table: HashTable<(PoolIdx, ObjIdx)>,
}

impl PoolIdxHashTable {
    fn get_with_hash(
        &self,
        pool_idx: PoolIdx,
        hash: u64,
        data: &[u8],
        obj_pool: &ObjectPool,
    ) -> Option<&(PoolIdx, ObjIdx)> {
        self.hash_table.find(hash, |val: &(PoolIdx, ObjIdx)| {
            cmp_pool_idx(pool_idx, val.0, data, obj_pool)
        })
    }

    fn set_with_hash(
        &mut self,
        pool_idx: PoolIdx,
        hash: u64,
        obj_idx: ObjIdx,
        data: &[u8],
        obj_pool: &ObjectPool,
    ) {
        let hasher = |val: &(PoolIdx, ObjIdx)| hash_one_pool_idx(val.0, data, obj_pool);

        self.hash_table
            .insert_unique(hash, (pool_idx, obj_idx), hasher);
    }

    fn del(&mut self, pool_idx: PoolIdx, hash: u64, data: &[u8], obj_pool: &ObjectPool) {
        let Ok(entry) = self.hash_table.find_entry(hash, |val: &(PoolIdx, ObjIdx)| {
            cmp_pool_idx(pool_idx, val.0, data, obj_pool)
        }) else {
            return;
        };
        entry.remove();
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

    #[test]
    fn test_hash_table() {
        let mut obj_pool = ObjectPool::default();
        let data: Vec<u8> = vec![
            0, 1, 2, 3, 4, 0, 1, 2, 3, 4, 4, 5, 6, 7, 8, 0, 1, 2, 3, 4, 0, 0, 0, 0, 0,
        ];
        let obj_0 = obj_pool.alloc();
        assert_eq!(obj_0, 0);
        let obj_1 = obj_pool.alloc();
        assert_eq!(obj_1, 1);
        let obj_2 = obj_pool.alloc();
        assert_eq!(obj_2, 2);
        let obj_3 = obj_pool.alloc();
        assert_eq!(obj_3, 3);

        //obj_0, obj_1 and obj_3 point to the same bytes
        {
            let obj_0 = obj_pool.get_obj_mut(0).unwrap();
            obj_0.head = 0;
            obj_0.tail = 5;
            let link = Link {
                width: LinkWidth::Two,
                is_signed: false,
                whence: OffsetWhence::Head,
                bias: 0,
                position: 21,
                objidx: Some(2),
            };
            obj_0.real_links.push(link);
        }

        {
            // obj_1 has the same real_links with obj_0
            let obj_1 = obj_pool.get_obj_mut(1).unwrap();
            obj_1.head = 5;
            obj_1.tail = 10;
            let link = Link {
                width: LinkWidth::Two,
                is_signed: false,
                whence: OffsetWhence::Head,
                bias: 0,
                position: 21,
                objidx: Some(2),
            };
            obj_1.real_links.push(link);
        }

        {
            let obj_2 = obj_pool.get_obj_mut(2).unwrap();
            obj_2.head = 10;
            obj_2.tail = 15;
        }

        {
            // obj_3 doesn't have real_links
            let obj_3 = obj_pool.get_obj_mut(3).unwrap();
            obj_3.head = 15;
            obj_3.tail = 20;
        }

        assert!(cmp_pool_idx(0, 1, &data, &obj_pool));
        assert!(!cmp_pool_idx(0, 2, &data, &obj_pool));
        assert!(!cmp_pool_idx(0, 3, &data, &obj_pool));
        assert!(!cmp_pool_idx(1, 3, &data, &obj_pool));

        let mut hash_table = PoolIdxHashTable::default();
        let hash_0 = hash_one_pool_idx(0, &data, &obj_pool);
        let hash_1 = hash_one_pool_idx(1, &data, &obj_pool);
        assert_eq!(hash_0, hash_1);
        let hash_2 = hash_one_pool_idx(2, &data, &obj_pool);
        let hash_3 = hash_one_pool_idx(3, &data, &obj_pool);
        assert_ne!(hash_0, hash_2);
        assert_ne!(hash_0, hash_3);
        assert_ne!(hash_2, hash_3);

        hash_table.set_with_hash(0, hash_0, 0, &data, &obj_pool);
        assert_eq!(
            hash_table.get_with_hash(1, hash_1, &data, &obj_pool),
            Some(&(0, 0))
        );
        assert_eq!(hash_table.get_with_hash(2, hash_2, &data, &obj_pool), None);
        assert_eq!(hash_table.get_with_hash(3, hash_3, &data, &obj_pool), None);

        hash_table.set_with_hash(2, hash_2, 2, &data, &obj_pool);
        assert_eq!(
            hash_table.get_with_hash(2, hash_2, &data, &obj_pool),
            Some(&(2, 2))
        );

        hash_table.set_with_hash(3, hash_3, 3, &data, &obj_pool);
        assert_eq!(
            hash_table.get_with_hash(3, hash_3, &data, &obj_pool),
            Some(&(3, 3))
        );

        // update obj_3 to have the same real links as obj_0
        {
            let obj_3 = obj_pool.get_obj_mut(3).unwrap();
            let link = Link {
                width: LinkWidth::Two,
                is_signed: false,
                whence: OffsetWhence::Head,
                bias: 0,
                position: 21,
                objidx: Some(2),
            };
            obj_3.real_links.push(link);
        }

        let hash_3_new = hash_one_pool_idx(3, &data, &obj_pool);
        assert_ne!(hash_3, hash_3_new);
        // old entries found with old hash
        assert_eq!(
            hash_table.get_with_hash(3, hash_3, &data, &obj_pool),
            Some(&(3, 3))
        );

        // test del
        hash_table.del(3, hash_3, &data, &obj_pool);
        assert_eq!(hash_table.get_with_hash(3, hash_3, &data, &obj_pool), None);

        // duplicate obj_0 entry found with new hash
        assert_eq!(
            hash_table.get_with_hash(3, hash_3_new, &data, &obj_pool),
            Some(&(0, 0))
        );
    }

    #[test]
    fn test_push_and_pop_pack() {
        let mut s = Serializer::new(100);
        let n = Uint24::new(80);
        assert_eq!(s.embed(n), Ok(0));
        assert_eq!(s.head, 3);
        assert_eq!(s.current, None);

        {
            //single pop_pack()
            assert_eq!(s.push(), Ok(()));
            //an object is allocated during push
            assert_eq!(s.current, Some(0));
            assert_eq!(s.embed(n), Ok(3));
            assert_eq!(s.head, 6);
            assert_eq!(s.tail, 100);
            assert_eq!(s.pop_pack(true), Some(1));
            assert_eq!(s.packed_map.hash_table.len(), 1);
            // TODO: avoid a single None object at the start
            assert_eq!(s.packed.len(), 2);
            assert_eq!(s.head, 3);
            assert_eq!(s.tail, 97);
        }

        {
            //test de-duplicate
            assert_eq!(s.push(), Ok(()));
            assert_eq!(s.current, Some(1));
            assert_eq!(s.embed(n), Ok(3));
            assert_eq!(s.head, 6);
            assert_eq!(s.pop_pack(true), Some(1));
            assert_eq!(s.packed_map.hash_table.len(), 1);
            // share=true, duplicate object won't be added into packed vector
            assert_eq!(s.packed.len(), 2);
            //check to make sure head is rewinded
            assert_eq!(s.head, 3);
            assert_eq!(s.tail, 97);

            assert_eq!(s.push(), Ok(()));
            // check that we released the previous duplicate object
            assert_eq!(s.current, Some(1));
            let n = UfWord::new(10);
            assert_eq!(s.embed(n), Ok(3));
            assert_eq!(s.head, 5);
            assert_eq!(s.pop_pack(true), Some(2));
            assert_eq!(s.packed_map.hash_table.len(), 2);
            assert_eq!(s.packed.len(), 3);
            //check to make sure head is rewinded
            assert_eq!(s.head, 3);
            //check that tail is updated
            assert_eq!(s.tail, 95);
        }

        {
            //test pop_pack(false) when duplicate objects exist
            assert_eq!(s.push(), Ok(()));
            assert_eq!(s.current, Some(2));
            let n = Uint24::new(80);
            assert_eq!(s.embed(n), Ok(3));
            assert_eq!(s.head, 6);
            assert_eq!(s.pop_pack(false), Some(3));
            // when share=false, we don't set this object in hash table, so the len of hash table is the same
            assert_eq!(s.packed_map.hash_table.len(), 2);
            // share=true, duplicate object will be added into the packed vector
            assert_eq!(s.packed.len(), 4);
            //check to make sure head is rewinded
            assert_eq!(s.head, 3);
            assert_eq!(s.tail, 92);
        }

        {
            // test de-duplicate with virtual links
            // virtual links don't affect equality and merge virtual links works
            assert_eq!(s.push(), Ok(()));
            assert_eq!(s.current, Some(3));
            let n = Uint24::new(123);
            assert_eq!(s.embed(n), Ok(3));
            assert_eq!(s.head, 6);

            //add virtual links
            let obj = s.object_pool.get_obj_mut(3).unwrap();
            let link = Link {
                width: LinkWidth::Two,
                is_signed: false,
                whence: OffsetWhence::Head,
                bias: 0,
                position: 0,
                objidx: Some(0),
            };
            obj.virtual_links.push(link);
            assert_eq!(s.pop_pack(true), Some(4));
            assert_eq!(s.packed_map.hash_table.len(), 3);
            assert_eq!(s.packed.len(), 5);
            //check to make sure head is rewinded
            assert_eq!(s.head, 3);
            assert_eq!(s.tail, 89);

            // another obj which differs only in virtual links
            assert_eq!(s.push(), Ok(()));
            assert_eq!(s.current, Some(4));
            let n = Uint24::new(123);
            assert_eq!(s.embed(n), Ok(3));
            assert_eq!(s.head, 6);

            //add virtual links
            let obj = s.object_pool.get_obj_mut(4).unwrap();
            let link = Link {
                width: LinkWidth::Two,
                is_signed: false,
                whence: OffsetWhence::Head,
                bias: 0,
                position: 0,
                objidx: Some(1),
            };
            obj.virtual_links.push(link);
            // check that virtual links doesn't affect euqality
            assert_eq!(s.pop_pack(true), Some(4));
            assert_eq!(s.packed_map.hash_table.len(), 3);
            assert_eq!(s.packed.len(), 5);
            assert_eq!(s.head, 3);
            assert_eq!(s.tail, 89);
            // check that duplicate obj is released, virtual links are emptied
            assert!(s.object_pool.get_obj(4).unwrap().virtual_links.is_empty());
            // check that merge_virtual_links works
            let obj = s.object_pool.get_obj_mut(3).unwrap();
            assert_eq!(obj.virtual_links.len(), 2);
            assert_eq!(obj.virtual_links[0].objidx, Some(0));
            assert_eq!(obj.virtual_links[1].objidx, Some(1));
        }

        {
            // test de-duplicate with real links
            // real links should be included in hash and equality computation
            assert_eq!(s.push(), Ok(()));
            assert_eq!(s.current, Some(4));
            let n = Uint24::new(321);
            assert_eq!(s.embed(n), Ok(3));
            assert_eq!(s.head, 6);

            //add real links
            let obj = s.object_pool.get_obj_mut(4).unwrap();
            let link = Link {
                width: LinkWidth::Two,
                is_signed: false,
                whence: OffsetWhence::Head,
                bias: 0,
                position: 10,
                objidx: Some(2),
            };
            obj.real_links.push(link);
            assert_eq!(s.pop_pack(true), Some(5));
            assert_eq!(s.packed_map.hash_table.len(), 4);
            assert_eq!(s.packed.len(), 6);
            //check to make sure head is rewinded
            assert_eq!(s.head, 3);
            assert_eq!(s.tail, 86);

            // another obj which differs only in real links
            assert_eq!(s.push(), Ok(()));
            assert_eq!(s.current, Some(5));
            let n = Uint24::new(321);
            assert_eq!(s.embed(n), Ok(3));
            assert_eq!(s.head, 6);

            //add real links
            let obj = s.object_pool.get_obj_mut(5).unwrap();
            let link = Link {
                width: LinkWidth::Two,
                is_signed: false,
                whence: OffsetWhence::Head,
                bias: 0,
                position: 20,
                objidx: Some(3),
            };
            obj.real_links.push(link);
            // pop_pack with a different obj_idx
            assert_eq!(s.pop_pack(true), Some(6));
            // obj is added into both hash_table and packed vector
            assert_eq!(s.packed_map.hash_table.len(), 5);
            assert_eq!(s.packed.len(), 7);
            //check to make sure head is rewinded
            assert_eq!(s.head, 3);
            assert_eq!(s.tail, 83);
        }
    }
}
