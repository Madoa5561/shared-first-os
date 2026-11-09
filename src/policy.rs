//! Optional, bounded immutable-page policy within one explicitly opted-in trust
//! domain. Tokens are kernel-private backing IDs, never physical addresses or
//! user authority. Object identities/revocation remain independent of dedup.
use crate::object::{Backend, BackendError};

pub const BYTES: usize = 4096;
const SLOTS: usize = 32;
const MAPS: usize = 64;
const PACKED: usize = 128;

pub trait Memory: Backend {
    /// Read an owned, resident frame without creating aliased Rust references.
    fn read_page(&self, frame: u64, output: &mut [u8; BYTES]);
    fn free_frames(&self) -> usize;
}

#[derive(Clone, Copy)]
struct Page {
    physical: u64, // zero iff compressed
    references: u32,
    mappings: usize,
    hash: u64,
    sealed: bool,
    private: bool,
    rejected: bool, // immutable, so retrying failed compression cannot help
    packed_length: usize,
    packed: [u8; PACKED],
}
#[derive(Clone, Copy)]
struct Mapping {
    task: usize,
    address: u64,
    token: u64,
}
#[derive(Default, Debug, Clone, Copy)]
pub struct Statistics {
    pub resident: usize,
    pub compressed_pages: usize,
    pub compressed_bytes: usize,
    pub mappings: usize,
    pub references: usize,
    pub metadata_bytes: usize,
    pub arena_bytes: usize,
    pub hits: usize,
    pub probes: usize,
    pub bypasses: usize,
    pub attempts: usize,
    pub rejected: usize,
    pub restored: usize,
}
pub struct Policy<B: Memory> {
    memory: B,
    pages: [Option<Page>; SLOTS],
    mappings: [Option<Mapping>; MAPS],
    enabled: bool,
    misses: usize,
    counters: Statistics,
}

/// Lossless byte-run encoding. Refuse a page unless the entire encoding fits;
/// incompressible data never replaces resident storage with an expanded copy.
fn encode(input: &[u8; BYTES], output: &mut [u8; PACKED]) -> Option<usize> {
    let mut offset = 0;
    let mut used = 0;
    while offset < BYTES {
        let byte = input[offset];
        let mut end = offset + 1;
        while end < BYTES && input[end] == byte {
            end += 1;
        }
        if used + 3 > output.len() {
            return None;
        }
        let count = (end - offset) as u16;
        output[used..used + 2].copy_from_slice(&count.to_le_bytes());
        output[used + 2] = byte;
        used += 3;
        offset = end;
    }
    Some(used)
}
fn decode(input: &[u8], output: &mut [u8; BYTES]) -> bool {
    if input.is_empty() || !input.len().is_multiple_of(3) {
        return false;
    }
    let mut offset = 0;
    for run in input.chunks_exact(3) {
        let count = u16::from_le_bytes([run[0], run[1]]) as usize;
        if count == 0 || offset + count > BYTES {
            return false;
        }
        output[offset..offset + count].fill(run[2]);
        offset += count;
    }
    offset == BYTES
}
fn fingerprint(input: &[u8; BYTES]) -> u64 {
    input.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ *byte as u64).wrapping_mul(0x100000001b3)
    })
}

impl<B: Memory> Policy<B> {
    pub const fn new(memory: B) -> Self {
        Self {
            memory,
            pages: [None; SLOTS],
            mappings: [None; MAPS],
            enabled: false,
            misses: 0,
            counters: Statistics {
                resident: 0,
                compressed_pages: 0,
                compressed_bytes: 0,
                mappings: 0,
                references: 0,
                metadata_bytes: 0,
                arena_bytes: 0,
                hits: 0,
                probes: 0,
                bypasses: 0,
                attempts: 0,
                rejected: 0,
                restored: 0,
            },
        }
    }
    /// Configure only an empty store. Backoff lasts for this explicit epoch.
    pub fn configure(&mut self, enabled: bool) {
        assert!(self.pages.iter().all(Option::is_none));
        assert!(self.mappings.iter().all(Option::is_none));
        self.enabled = enabled;
        self.misses = 0;
        self.counters = Statistics::default();
    }
    fn index(&self, token: u64) -> usize {
        let index = usize::try_from(token).unwrap().checked_sub(1).unwrap();
        assert!(self.pages.get(index).is_some_and(Option::is_some));
        index
    }
    fn read(&self, index: usize, output: &mut [u8; BYTES]) {
        let page = self.pages[index].as_ref().unwrap();
        if page.physical != 0 {
            self.memory.read_page(page.physical, output);
        } else {
            assert!(decode(&page.packed[..page.packed_length], output));
        }
    }
    fn resident(&mut self, index: usize) -> Result<u64, BackendError> {
        let page = self.pages[index].unwrap();
        if page.physical != 0 {
            return Ok(page.physical);
        }
        let mut bytes = [0; BYTES];
        self.read(index, &mut bytes);
        let frame = self.memory.allocate(&bytes).ok_or(BackendError::NoMemory)?;
        if let Err(error) = self.memory.seal(frame) {
            self.memory.destroy(frame);
            return Err(error); // compressed data remains authoritative on failure
        }
        let entry = self.pages[index].as_mut().unwrap();
        entry.physical = frame;
        entry.packed_length = 0;
        entry.packed.fill(0);
        self.counters.restored += 1;
        Ok(frame)
    }
    /// Caller supplies a kernel-owned watermark, never a user syscall value.
    /// Compress only cold sealed pages; all mapped and private pages stay pinned.
    pub fn maintain(&mut self, low_watermark: usize) {
        if !self.enabled || self.memory.free_frames() > low_watermark {
            return;
        }
        for index in 0..SLOTS {
            if self.memory.free_frames() > low_watermark {
                break;
            }
            let Some(page) = self.pages[index] else {
                continue;
            };
            if !page.sealed
                || page.private
                || page.mappings != 0
                || page.physical == 0
                || page.rejected
            {
                continue;
            }
            let mut bytes = [0; BYTES];
            let mut packed = [0; PACKED];
            self.read(index, &mut bytes);
            self.counters.attempts += 1;
            if let Some(length) = encode(&bytes, &mut packed) {
                self.memory.destroy(page.physical);
                let entry = self.pages[index].as_mut().unwrap();
                entry.physical = 0;
                entry.packed = packed;
                entry.packed_length = length;
            } else {
                self.pages[index].as_mut().unwrap().rejected = true;
                self.counters.rejected += 1;
            }
        }
    }
    pub fn stats(&self) -> Statistics {
        let mut stats = self.counters;
        stats.arena_bytes = SLOTS * PACKED;
        // Reserved arena reported separately; both are already in kernel storage.
        stats.metadata_bytes =
            core::mem::size_of::<Self>() - core::mem::size_of::<B>() - stats.arena_bytes;
        stats.mappings = self.mappings.iter().flatten().count();
        for page in self.pages.iter().flatten() {
            stats.references += page.references as usize;
            stats.resident += usize::from(page.physical != 0);
            stats.compressed_pages += usize::from(page.physical == 0);
            stats.compressed_bytes += page.packed_length;
        }
        stats
    }
}

impl<B: Memory> Backend for Policy<B> {
    fn resident_payload_frames(&self, _logical: usize) -> usize {
        self.stats().resident
    }
    fn allocate(&mut self, data: &[u8]) -> Option<u64> {
        assert!(data.len() <= BYTES);
        let mut bytes = [0; BYTES];
        bytes[..data.len()].copy_from_slice(data);
        let hash = if self.enabled && self.misses < 8 {
            fingerprint(&bytes)
        } else {
            0
        };
        if self.enabled && self.misses < 8 {
            for index in 0..SLOTS {
                let Some(page) = self.pages[index] else {
                    continue;
                };
                if !page.sealed || page.private || page.hash != hash {
                    continue;
                }
                self.counters.probes += 1;
                let mut candidate = [0; BYTES];
                self.read(index, &mut candidate);
                if candidate == bytes && page.references < u32::MAX {
                    self.pages[index].as_mut().unwrap().references += 1;
                    self.counters.hits += 1;
                    self.misses = 0;
                    return Some(index as u64 + 1);
                }
            }
            self.misses += 1;
        } else if self.enabled {
            self.counters.bypasses += 1;
        }
        let index = self.pages.iter().position(Option::is_none)?;
        let physical = self.memory.allocate(data)?;
        self.pages[index] = Some(Page {
            physical,
            references: 1,
            mappings: 0,
            hash,
            sealed: false,
            private: false,
            rejected: false,
            packed_length: 0,
            packed: [0; PACKED],
        });
        Some(index as u64 + 1)
    }
    fn seal(&mut self, token: u64) -> Result<(), BackendError> {
        let index = self.index(token);
        let page = self.pages[index].unwrap();
        if !page.sealed {
            assert!(!page.private);
            self.memory.seal(page.physical)?;
            self.pages[index].as_mut().unwrap().sealed = true;
        }
        Ok(())
    }
    fn destroy(&mut self, token: u64) {
        let index = self.index(token);
        let page = self.pages[index].as_mut().unwrap();
        assert!(page.references > 0);
        page.references -= 1;
        if page.references == 0 {
            assert_eq!(page.mappings, 0);
            let frame = page.physical;
            self.pages[index] = None;
            if frame != 0 {
                self.memory.destroy(frame);
            }
        }
    }
    fn valid_range(&self, task: usize, address: u64, pages: usize) -> bool {
        self.memory.valid_range(task, address, pages)
    }
    fn map_readonly(&mut self, task: usize, address: u64, token: u64) -> Result<(), BackendError> {
        if self
            .mappings
            .iter()
            .flatten()
            .any(|m| m.task == task && m.address == address)
        {
            return Err(BackendError::Conflict);
        }
        let map = self
            .mappings
            .iter()
            .position(Option::is_none)
            .ok_or(BackendError::NoMemory)?;
        let index = self.index(token);
        assert!(self.pages[index].unwrap().sealed);
        let physical = self.resident(index)?;
        self.memory.map_readonly(task, address, physical)?;
        self.mappings[map] = Some(Mapping {
            task,
            address,
            token,
        });
        self.pages[index].as_mut().unwrap().mappings += 1;
        Ok(())
    }
    fn unmap(&mut self, task: usize, address: u64) -> u64 {
        let map = self
            .mappings
            .iter()
            .position(|m| m.is_some_and(|m| m.task == task && m.address == address))
            .unwrap();
        let mapping = self.mappings[map].take().unwrap();
        let index = self.index(mapping.token);
        let page = self.pages[index].as_mut().unwrap();
        assert_eq!(self.memory.unmap(task, address), page.physical);
        page.mappings -= 1;
        mapping.token
    }
    fn copy_private(&mut self, original: u64) -> Option<u64> {
        let source = self.index(original);
        let index = self.pages.iter().position(Option::is_none)?;
        let physical = self.resident(source).ok()?;
        let copy = self.memory.copy_private(physical)?;
        self.pages[index] = Some(Page {
            physical: copy,
            references: 1,
            mappings: 0,
            hash: 0,
            sealed: false,
            private: true,
            rejected: false,
            packed_length: 0,
            packed: [0; PACKED],
        });
        Some(index as u64 + 1)
    }
    fn replace_private(
        &mut self,
        task: usize,
        address: u64,
        old: u64,
        new: u64,
    ) -> Result<(), BackendError> {
        let map = self
            .mappings
            .iter()
            .position(|m| {
                m.is_some_and(|m| m.task == task && m.address == address && m.token == old)
            })
            .ok_or(BackendError::Conflict)?;
        let source = self.index(old);
        let target = self.index(new);
        let original = self.pages[source].unwrap();
        let copy = self.pages[target].unwrap();
        assert!(original.sealed && copy.private && copy.mappings == 0);
        self.memory
            .replace_private(task, address, original.physical, copy.physical)?;
        self.pages[source].as_mut().unwrap().mappings -= 1;
        self.pages[target].as_mut().unwrap().mappings += 1;
        self.mappings[map].as_mut().unwrap().token = new;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::object::{ObjectStore, Rights};
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Model {
        frames: BTreeMap<u64, [u8; BYTES]>,
        maps: BTreeMap<(usize, u64), u64>,
        next: u64,
        fail_allocate: bool,
        fail_seal: bool,
        fail_map: bool,
        fail_replace: bool,
    }
    impl Backend for Model {
        fn allocate(&mut self, data: &[u8]) -> Option<u64> {
            if self.fail_allocate {
                return None;
            }
            self.next += 4096;
            let mut bytes = [0; BYTES];
            bytes[..data.len()].copy_from_slice(data);
            self.frames.insert(self.next, bytes);
            Some(self.next)
        }
        fn seal(&mut self, _: u64) -> Result<(), BackendError> {
            if self.fail_seal {
                Err(BackendError::NoMemory)
            } else {
                Ok(())
            }
        }
        fn destroy(&mut self, frame: u64) {
            assert!(!self.maps.values().any(|&value| value == frame));
            assert!(self.frames.remove(&frame).is_some());
        }
        fn valid_range(&self, _: usize, _: u64, _: usize) -> bool {
            true
        }
        fn map_readonly(
            &mut self,
            task: usize,
            address: u64,
            frame: u64,
        ) -> Result<(), BackendError> {
            if self.fail_map {
                return Err(BackendError::NoMemory);
            }
            if self.maps.contains_key(&(task, address)) {
                return Err(BackendError::Conflict);
            }
            self.maps.insert((task, address), frame);
            Ok(())
        }
        fn unmap(&mut self, task: usize, address: u64) -> u64 {
            self.maps.remove(&(task, address)).unwrap()
        }
        fn copy_private(&mut self, original: u64) -> Option<u64> {
            let bytes = self.frames[&original];
            self.allocate(&bytes)
        }
        fn replace_private(
            &mut self,
            task: usize,
            address: u64,
            old: u64,
            new: u64,
        ) -> Result<(), BackendError> {
            if self.fail_replace {
                return Err(BackendError::Conflict);
            }
            assert_eq!(self.maps.insert((task, address), new), Some(old));
            Ok(())
        }
    }
    impl Memory for Model {
        fn read_page(&self, frame: u64, output: &mut [u8; BYTES]) {
            *output = self.frames[&frame];
        }
        fn free_frames(&self) -> usize {
            100 - self.frames.len()
        }
    }
    fn policy() -> Policy<Model> {
        let mut result = Policy::new(Model::default());
        result.configure(true);
        result
    }
    fn insert(policy: &mut Policy<Model>, value: u8) -> u64 {
        let token = policy.allocate(&[value; BYTES]).unwrap();
        policy.seal(token).unwrap();
        token
    }
    #[test]
    fn codec_roundtrip_rejects_random_and_malformed_runs() {
        let mut output = [0; PACKED];
        let mut restored = [0; BYTES];
        for byte in [0, 1, 127, 255] {
            let page = [byte; BYTES];
            assert_eq!(encode(&page, &mut output), Some(3));
            assert!(decode(&output[..3], &mut restored));
            assert_eq!(page, restored);
        }
        let page = core::array::from_fn(|index| index as u8);
        assert_eq!(encode(&page, &mut output), None);
        for input in [&[][..], &[0, 0, 4], &[1, 0, 4], &[0, 32, 4], &[0, 16]] {
            assert!(!decode(input, &mut restored));
        }
        let mut page = [9; BYTES];
        page[100..3000].fill(3);
        let length = encode(&page, &mut output).unwrap();
        assert!(decode(&output[..length], &mut restored));
        assert_eq!(page, restored);
    }
    #[test]
    fn dedup_is_not_object_identity_and_revoke_preserves_other_object() {
        let mut store = ObjectStore::<_, 4>::new(policy());
        let first = store.create_filled(0, BYTES * 4, 7).unwrap();
        let second = store.create_filled(1, BYTES * 4, 7).unwrap();
        assert_eq!(store.stats().payload_frames, 1);
        store.map(0, first, 0x1000, Rights::READ).unwrap();
        store.map(1, second, 0x1000, Rights::READ).unwrap();
        store.revoke(0, first).unwrap();
        assert_eq!(store.stats().payload_frames, 1);
        assert_eq!(store.backend().stats().mappings, 4);
        assert_eq!(store.length(1, second), Ok(BYTES * 4));
        store.release(1, second).unwrap();
        assert_eq!(store.backend().memory.frames.len(), 0);
    }
    #[test]
    fn compressed_dedup_and_restore_failure_keep_authoritative_bytes() {
        let mut policy = policy();
        let first = insert(&mut policy, 0x5a);
        policy.maintain(99);
        assert_eq!(policy.stats().resident, 0);
        assert_eq!(policy.stats().compressed_bytes, 3);
        assert_eq!(insert(&mut policy, 0x5a), first);
        policy.memory.fail_allocate = true;
        assert_eq!(
            policy.map_readonly(0, 0x1000, first),
            Err(BackendError::NoMemory)
        );
        assert_eq!(policy.stats().compressed_bytes, 3);
        policy.memory.fail_allocate = false;
        policy.memory.fail_seal = true;
        assert_eq!(
            policy.map_readonly(0, 0x1000, first),
            Err(BackendError::NoMemory)
        );
        assert!(policy.memory.frames.is_empty());
        policy.memory.fail_seal = false;
        policy.memory.fail_map = true;
        assert_eq!(
            policy.map_readonly(0, 0x1000, first),
            Err(BackendError::NoMemory)
        );
        assert_eq!(policy.stats().mappings, 0);
        policy.memory.fail_map = false;
        policy.map_readonly(0, 0x1000, first).unwrap();
        let physical = policy.memory.maps[&(0, 0x1000)];
        assert_eq!(policy.memory.frames[&physical], [0x5a; BYTES]);
        policy.unmap(0, 0x1000);
        policy.destroy(first);
        policy.destroy(first);
        assert!(policy.memory.frames.is_empty());
    }
    #[test]
    fn pressure_pins_maps_and_private_pages_and_cow_rollback() {
        let mut store = ObjectStore::<_, 4>::new(policy());
        let cap = store.create_filled(0, BYTES, 0x33).unwrap();
        store.map(0, cap, 0x1000, Rights::COW).unwrap();
        store.backend_mut().maintain(100);
        assert_eq!(store.backend().stats().attempts, 0);
        store.backend_mut().memory.fail_replace = true;
        assert_eq!(
            store.resolve_cow(0, 0x1000),
            Err(crate::object::Error::Backend(BackendError::Conflict))
        );
        assert_eq!(store.stats().payload_frames, 1);
        store.backend_mut().memory.fail_replace = false;
        store.resolve_cow(0, 0x1000).unwrap();
        store.backend_mut().maintain(100); // original cold, private mapped
        assert_eq!(store.backend().stats().compressed_pages, 1);
        assert_eq!(store.backend().stats().resident, 1);
        store.release(0, cap).unwrap();
        assert!(store.backend().memory.frames.is_empty());
    }
    #[test]
    fn hash_collision_requires_full_comparison_and_backoff_is_bounded() {
        let mut policy = policy();
        let first = insert(&mut policy, 1);
        policy.pages[0].as_mut().unwrap().hash = fingerprint(&[2; BYTES]);
        let second = insert(&mut policy, 2);
        assert_ne!(first, second);
        assert_eq!(policy.stats().hits, 0);
        assert_eq!(policy.stats().probes, 1);
        for byte in 3..=16 {
            insert(&mut policy, byte);
        }
        assert_eq!(policy.stats().bypasses, 8);
        for token in 1..=16 {
            policy.destroy(token);
        }
        policy.configure(false);
        assert_ne!(insert(&mut policy, 9), insert(&mut policy, 9));
        policy.maintain(100);
        assert_eq!(policy.stats().attempts, 0);
    }
    #[test]
    fn threshold_rejection_and_capacity_do_not_leak() {
        let mut policy = policy();
        let token = policy
            .allocate(&core::array::from_fn::<_, BYTES, _>(|i| i as u8))
            .unwrap();
        policy.seal(token).unwrap();
        policy.maintain(98);
        assert_eq!(policy.stats().attempts, 0);
        policy.maintain(99);
        policy.maintain(100);
        assert_eq!(policy.stats().attempts, 1);
        assert_eq!(policy.stats().rejected, 1);
        for value in 1..32 {
            insert(&mut policy, value);
        }
        assert_eq!(policy.allocate(&[33; BYTES]), None);
        assert_eq!(policy.stats().resident, 32);
        for token in 1..=32 {
            policy.destroy(token);
        }
        assert!(policy.memory.frames.is_empty());
    }
}
