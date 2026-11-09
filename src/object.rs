//! Bounded immutable objects. Callers are trusted kernel code: `task` must come
//! from the scheduler, never from an untrusted syscall argument. A handle is
//! interpreted only in that task's table. No object ID is an authority.
//! Single CPU, serialized operations; the backend owns mapping/TLB enforcement.
use crate::frame::PAGE_SIZE;

pub const TASKS: usize = 4;
pub const CAPS: usize = 16;
pub const OBJECTS: usize = 8;
pub const PAGES: usize = 16;
pub const VIEWS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handle {
    pub slot: u16,
    pub generation: u32,
}
impl Handle {
    pub const fn encode(self) -> u64 {
        ((self.generation as u64) << 16) | self.slot as u64
    }
    pub fn decode(value: u64) -> Result<Self, Error> {
        if value >> 48 != 0 || (value >> 16) == 0 {
            return Err(Error::StaleHandle);
        }
        Ok(Self {
            slot: value as u16,
            generation: (value >> 16) as u32,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rights(u8);
impl Rights {
    pub const READ: Self = Self(1);
    pub const MAP: Self = Self(2);
    pub const SHARE: Self = Self(4);
    pub const REVOKE: Self = Self(8);
    pub const WRITE: Self = Self(16);
    pub const EXECUTE: Self = Self(32);
    pub const COW: Self = Self(64);
    pub const READ_MAP: Self = Self(3);
    pub const OWNER: Self = Self(79);

    pub fn from_bits(bits: u64) -> Result<Self, Error> {
        if bits == 0 || bits & !127 != 0 {
            return Err(Error::Denied);
        }
        Ok(Self(bits as u8))
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendError {
    NoMemory,
    Conflict,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidTask,
    StaleHandle,
    Denied,
    Capacity,
    InvalidSize,
    Overflow,
    InvalidAddress,
    NotMapped,
    Backend(BackendError),
}

/// Trusted memory boundary. allocate initializes a private page and zeros its
/// entire tail. seal removes ALL writable aliases and invalidates translations;
/// failure must leave the page private. map_readonly is atomic on failure and
/// creates only R/NX mappings. unmap removes the PTE and invalidates the TLB
/// before returning. destroy is infallible for owned frames: after all views
/// are gone it restores the private alias, zeros the page and returns ownership.
/// No backend operation may create Rust references aliasing these raw pages.
pub trait Backend {
    /// Resident physical backing, distinct from logical references when an
    /// optional backend interns or compresses immutable pages.
    fn resident_payload_frames(&self, logical: usize) -> usize {
        logical
    }
    fn allocate(&mut self, data: &[u8]) -> Option<u64>;
    fn seal(&mut self, frame: u64) -> Result<(), BackendError>;
    fn destroy(&mut self, frame: u64);
    fn valid_range(&self, task: usize, address: u64, pages: usize) -> bool;
    fn map_readonly(&mut self, task: usize, address: u64, frame: u64) -> Result<(), BackendError>;
    fn unmap(&mut self, task: usize, address: u64) -> u64;
    /// Return a private RW/NX copy, or fail without retaining a frame.
    fn copy_private(&mut self, _original: u64) -> Option<u64> {
        None
    }
    /// Replace only the expected RO leaf with a private RW/NX page. Atomic on
    /// failure; invalidate before returning. Never modify the immutable source.
    fn replace_private(
        &mut self,
        _task: usize,
        _address: u64,
        _old: u64,
        _new: u64,
    ) -> Result<(), BackendError> {
        Err(BackendError::Conflict)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Key {
    slot: usize,
    generation: u32,
}

/// Optional trusted, synchronous access to retained immutable backing. A caller
/// must supply an owned sealed frame and a checked range within that page. No
/// reference or native address escapes to the bytecode consumer.
pub trait ReadBackend: Backend {
    fn sum_page(&self, frame: u64, offset: usize, length: usize) -> u64;
}
#[derive(Clone, Copy)]
struct Descriptor<const N: usize> {
    frames: [u64; N],
    pages: usize,
    length: usize,
    // Each view is owned by a live capability, removed before releasing that
    // capability. Therefore counting capabilities also pins all mapped pages.
    references: u32,
}
#[derive(Clone, Copy)]
struct ObjectSlot<const N: usize> {
    generation: u32,
    value: Option<Descriptor<N>>,
}
#[derive(Clone, Copy)]
struct Authority {
    key: Key,
    rights: Rights,
}
#[derive(Clone, Copy)]
struct CapSlot {
    generation: u32,
    value: Option<Authority>,
}
#[derive(Clone, Copy)]
struct View {
    task: usize,
    handle: Handle,
    address: u64,
    cow: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Stats {
    pub objects: usize,
    pub capabilities: usize,
    pub payload_frames: usize,
    pub private_frames: usize,
    pub mapping_pages: usize,
    pub metadata_bytes: usize,
}

pub type Store<B> = ObjectStore<B, PAGES>;

pub struct ObjectStore<B: Backend, const N: usize> {
    backend: B,
    objects: [ObjectSlot<N>; OBJECTS],
    caps: [[CapSlot; CAPS]; TASKS],
    views: [Option<View>; VIEWS],
    private: [[u64; N]; VIEWS], // Zero means the view still uses immutable backing.
}

pub fn page_count(length: usize) -> Result<usize, Error> {
    count_pages(length, PAGES)
}

fn count_pages(length: usize, maximum: usize) -> Result<usize, Error> {
    let pages = length
        .checked_add(PAGE_SIZE as usize - 1)
        .ok_or(Error::Overflow)?
        / PAGE_SIZE as usize;
    if pages == 0 || pages > maximum {
        return Err(Error::InvalidSize);
    }
    Ok(pages)
}

impl<B: Backend, const N: usize> ObjectStore<B, N> {
    pub const fn new(backend: B) -> Self {
        assert!(N > 0 && N <= 1024);
        Self {
            backend,
            objects: [ObjectSlot {
                generation: 1,
                value: None,
            }; OBJECTS],
            caps: [[CapSlot {
                generation: 1,
                value: None,
            }; CAPS]; TASKS],
            views: [None; VIEWS],
            private: [[0; N]; VIEWS],
        }
    }
    pub fn backend(&self) -> &B {
        &self.backend
    }
    /// Trusted policy maintenance only; no store operation or user execution may
    /// overlap this borrow. The backend must retain every live backing token.
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }
    fn free_cap(&self, task: usize) -> Result<usize, Error> {
        self.caps
            .get(task)
            .ok_or(Error::InvalidTask)?
            .iter()
            .position(|cap| cap.value.is_none() && cap.generation != 0)
            .ok_or(Error::Capacity)
    }
    fn authority(&self, task: usize, handle: Handle) -> Result<Authority, Error> {
        let table = self.caps.get(task).ok_or(Error::InvalidTask)?;
        let slot = table.get(handle.slot as usize).ok_or(Error::StaleHandle)?;
        if slot.generation != handle.generation || handle.generation == 0 {
            return Err(Error::StaleHandle);
        }
        let authority = slot.value.ok_or(Error::StaleHandle)?;
        let object = &self.objects[authority.key.slot];
        assert_eq!(object.generation, authority.key.generation);
        assert!(object.value.is_some());
        Ok(authority)
    }
    fn install(&mut self, task: usize, slot: usize, authority: Authority) -> Handle {
        assert!(self.caps[task][slot].value.is_none());
        self.caps[task][slot].value = Some(authority);
        Handle {
            slot: slot as u16,
            generation: self.caps[task][slot].generation,
        }
    }
    pub fn create(&mut self, task: usize, data: &[u8]) -> Result<Handle, Error> {
        self.create_initialized(task, data.len(), |offset, page| {
            page.copy_from_slice(&data[offset..offset + page.len()]);
        })
    }
    /// Bounded fixture/data initializer, avoiding a second multi-MiB input buffer.
    pub fn create_filled(&mut self, task: usize, length: usize, byte: u8) -> Result<Handle, Error> {
        self.create_initialized(task, length, |_, page| page.fill(byte))
    }
    /// Kernel-only bounded initializer; the callback may initialize bytes but
    /// must not reenter the store or expose the temporary buffer to user code.
    pub fn create_initialized(
        &mut self,
        task: usize,
        length: usize,
        mut initialize: impl FnMut(usize, &mut [u8]),
    ) -> Result<Handle, Error> {
        let pages = count_pages(length, N)?;
        let cap = self.free_cap(task)?;
        let slot = self
            .objects
            .iter()
            .position(|object| object.value.is_none() && object.generation != 0)
            .ok_or(Error::Capacity)?;
        let mut frames = [0; N];
        let mut buffer = [0; PAGE_SIZE as usize];
        for index in 0..pages {
            let start = index * PAGE_SIZE as usize;
            let size = (length - start).min(PAGE_SIZE as usize);
            initialize(start, &mut buffer[..size]);
            let Some(frame) = self.backend.allocate(&buffer[..size]) else {
                for &frame in &frames[..index] {
                    self.backend.destroy(frame);
                }
                return Err(Error::Backend(BackendError::NoMemory));
            };
            frames[index] = frame;
            if let Err(error) = self.backend.seal(frame) {
                for &frame in &frames[..=index] {
                    self.backend.destroy(frame);
                }
                return Err(Error::Backend(error));
            }
        }
        let key = Key {
            slot,
            generation: self.objects[slot].generation,
        };
        self.objects[slot].value = Some(Descriptor {
            frames,
            pages,
            length,
            references: 1,
        });
        Ok(self.install(
            task,
            cap,
            Authority {
                key,
                rights: Rights::OWNER,
            },
        ))
    }
    pub fn length(&self, task: usize, handle: Handle) -> Result<usize, Error> {
        let authority = self.authority(task, handle)?;
        if !authority.rights.contains(Rights::READ) {
            return Err(Error::Denied);
        }
        Ok(self.objects[authority.key.slot].value.unwrap().length)
    }
    /// Mediated immutable read, independent of native mappings or COW views.
    /// The trusted scheduler supplies task; authorization is rechecked on every
    /// call, including after a bytecode yield. No borrow spans a revoke/release.
    pub fn read_sum(
        &self,
        task: usize,
        handle: Handle,
        offset: u64,
        length: u64,
    ) -> Result<u64, Error>
    where
        B: ReadBackend,
    {
        let authority = self.authority(task, handle)?;
        if !authority.rights.contains(Rights::READ) {
            return Err(Error::Denied);
        }
        let object = self.objects[authority.key.slot].value.as_ref().unwrap();
        let end = offset.checked_add(length).ok_or(Error::Overflow)?;
        if end > object.length as u64 {
            return Err(Error::InvalidAddress);
        }
        let mut cursor = offset as usize;
        let mut sum = 0;
        while cursor < end as usize {
            let index = cursor / PAGE_SIZE as usize;
            let within = cursor % PAGE_SIZE as usize;
            let count = (end as usize - cursor).min(PAGE_SIZE as usize - within);
            sum += self.backend.sum_page(object.frames[index], within, count);
            cursor += count;
        }
        Ok(sum)
    }
    pub fn grant(
        &mut self,
        task: usize,
        handle: Handle,
        target: usize,
        rights: Rights,
    ) -> Result<Handle, Error> {
        let authority = self.authority(task, handle)?;
        if !authority.rights.contains(Rights::SHARE)
            || rights.0 == 0
            || !authority.rights.contains(rights)
        {
            return Err(Error::Denied);
        }
        let slot = self.free_cap(target)?;
        let object = self.objects[authority.key.slot].value.as_mut().unwrap();
        let references = object.references.checked_add(1).ok_or(Error::Overflow)?;
        object.references = references;
        Ok(self.install(
            target,
            slot,
            Authority {
                key: authority.key,
                rights,
            },
        ))
    }
    pub fn map(
        &mut self,
        task: usize,
        handle: Handle,
        address: u64,
        access: Rights,
    ) -> Result<(), Error> {
        let authority = self.authority(task, handle)?;
        if !authority.rights.contains(Rights::READ_MAP)
            || (access != Rights::READ && access != Rights::COW)
            || (access == Rights::COW && !authority.rights.contains(Rights::COW))
        {
            return Err(Error::Denied);
        }
        let object = self.objects[authority.key.slot].value.unwrap();
        if !address.is_multiple_of(PAGE_SIZE)
            || address
                .checked_add(object.pages as u64 * PAGE_SIZE)
                .is_none()
            || !self.backend.valid_range(task, address, object.pages)
        {
            return Err(Error::InvalidAddress);
        }
        let slot = self
            .views
            .iter()
            .position(Option::is_none)
            .ok_or(Error::Capacity)?;
        for index in 0..object.pages {
            if let Err(error) = self.backend.map_readonly(
                task,
                address + index as u64 * PAGE_SIZE,
                object.frames[index],
            ) {
                for previous in (0..index).rev() {
                    assert_eq!(
                        self.backend
                            .unmap(task, address + previous as u64 * PAGE_SIZE),
                        object.frames[previous]
                    );
                }
                return Err(Error::Backend(error));
            }
        }
        self.views[slot] = Some(View {
            task,
            handle,
            address,
            cow: access == Rights::COW,
        });
        Ok(())
    }
    fn remove_view(&mut self, slot: usize) {
        let view = self.views[slot].unwrap();
        let authority = self.authority(view.task, view.handle).unwrap();
        let object = self.objects[authority.key.slot].value.unwrap();
        for index in (0..object.pages).rev() {
            let private = self.private[slot][index];
            assert_eq!(
                self.backend
                    .unmap(view.task, view.address + index as u64 * PAGE_SIZE),
                if private == 0 {
                    object.frames[index]
                } else {
                    private
                }
            );
            if private != 0 {
                self.backend.destroy(private);
                self.private[slot][index] = 0;
            }
        }
        self.views[slot] = None;
    }
    /// Called only for a hardware present/user/write fault. Hardware classification
    /// belongs to the trap layer; this layer independently rechecks live authority.
    pub fn resolve_cow(&mut self, task: usize, address: u64) -> Result<(), Error> {
        if task >= TASKS {
            return Err(Error::InvalidTask);
        }
        let page = address & !(PAGE_SIZE - 1);
        let slot = self
            .views
            .iter()
            .position(|view| {
                view.is_some_and(|view| {
                    if view.task != task || !view.cow || page < view.address {
                        return false;
                    }
                    let authority = self.authority(task, view.handle).unwrap();
                    page - view.address
                        < self.objects[authority.key.slot].value.unwrap().pages as u64 * PAGE_SIZE
                })
            })
            .ok_or(Error::Denied)?;
        let view = self.views[slot].unwrap();
        let authority = self.authority(task, view.handle)?;
        if !authority
            .rights
            .contains(Rights::READ_MAP.union(Rights::COW))
        {
            return Err(Error::Denied);
        }
        let index = ((page - view.address) / PAGE_SIZE) as usize;
        if self.private[slot][index] != 0 {
            return Err(Error::Denied);
        }
        let original = self.objects[authority.key.slot].value.unwrap().frames[index];
        let private = self
            .backend
            .copy_private(original)
            .ok_or(Error::Backend(BackendError::NoMemory))?;
        if let Err(error) = self.backend.replace_private(task, page, original, private) {
            self.backend.destroy(private);
            return Err(Error::Backend(error));
        }
        self.private[slot][index] = private;
        Ok(())
    }

    /// Task termination removes all local capabilities, views and private copies.
    pub fn release_task(&mut self, task: usize) -> Result<(), Error> {
        if task >= TASKS {
            return Err(Error::InvalidTask);
        }
        for slot in 0..CAPS {
            let cap = self.caps[task][slot];
            if cap.value.is_some() {
                self.release(
                    task,
                    Handle {
                        slot: slot as u16,
                        generation: cap.generation,
                    },
                )?;
            }
        }
        Ok(())
    }
    pub fn unmap(&mut self, task: usize, handle: Handle, address: u64) -> Result<(), Error> {
        self.authority(task, handle)?;
        let slot = self
            .views
            .iter()
            .position(|view| {
                view.is_some_and(|view| {
                    view.task == task && view.handle == handle && view.address == address
                })
            })
            .ok_or(Error::NotMapped)?;
        self.remove_view(slot);
        Ok(())
    }
    pub fn release(&mut self, task: usize, handle: Handle) -> Result<(), Error> {
        let authority = self.authority(task, handle)?;
        for slot in 0..VIEWS {
            if self.views[slot].is_some_and(|view| view.task == task && view.handle == handle) {
                self.remove_view(slot);
            }
        }
        let cap = &mut self.caps[task][handle.slot as usize];
        cap.value = None;
        cap.generation = cap.generation.checked_add(1).unwrap_or(0); // 0 = retired.
        let slot = &mut self.objects[authority.key.slot];
        let object = slot.value.as_mut().unwrap();
        object.references = object
            .references
            .checked_sub(1)
            .expect("reference underflow");
        if object.references == 0 {
            for &frame in &object.frames[..object.pages] {
                self.backend.destroy(frame);
            }
            slot.value = None;
            slot.generation = slot.generation.checked_add(1).unwrap_or(0);
        }
        Ok(())
    }
    /// Object-wide revocation: all delegated capabilities and their views go
    /// away, including the caller's. This is not subtree-only revocation.
    pub fn revoke(&mut self, task: usize, handle: Handle) -> Result<(), Error> {
        let authority = self.authority(task, handle)?;
        if !authority.rights.contains(Rights::REVOKE) {
            return Err(Error::Denied);
        }
        for target in 0..TASKS {
            for slot in 0..CAPS {
                let cap = self.caps[target][slot];
                if cap.value.is_some_and(|entry| entry.key == authority.key) {
                    self.release(
                        target,
                        Handle {
                            slot: slot as u16,
                            generation: cap.generation,
                        },
                    )?;
                }
            }
        }
        Ok(())
    }
    pub fn stats(&self) -> Stats {
        let private_frames = self
            .private
            .iter()
            .flatten()
            .filter(|&&frame| frame != 0)
            .count();
        Stats {
            private_frames,
            objects: self
                .objects
                .iter()
                .filter(|slot| slot.value.is_some())
                .count(),
            capabilities: self
                .caps
                .iter()
                .flatten()
                .filter(|slot| slot.value.is_some())
                .count(),
            payload_frames: self.backend.resident_payload_frames(
                self.objects
                    .iter()
                    .filter_map(|slot| slot.value)
                    .map(|o| o.pages)
                    .sum::<usize>()
                    + private_frames,
            ),
            mapping_pages: self
                .views
                .iter()
                .flatten()
                .map(|view| {
                    let cap = self.authority(view.task, view.handle).unwrap();
                    self.objects[cap.key.slot].value.unwrap().pages
                })
                .sum(),
            metadata_bytes: core::mem::size_of_val(&self.objects)
                + core::mem::size_of_val(&self.caps)
                + core::mem::size_of_val(&self.views)
                + core::mem::size_of_val(&self.private),
        }
    }
}

impl<B: Backend, const N: usize> Drop for ObjectStore<B, N> {
    fn drop(&mut self) {
        for task in 0..TASKS {
            for slot in 0..CAPS {
                let cap = self.caps[task][slot];
                if cap.value.is_some() {
                    self.release(
                        task,
                        Handle {
                            slot: slot as u16,
                            generation: cap.generation,
                        },
                    )
                    .expect("store teardown");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::{collections::BTreeMap, vec, vec::Vec};

    #[derive(Default)]
    struct Model {
        frames: BTreeMap<u64, (Vec<u8>, bool)>,
        mappings: BTreeMap<(usize, u64), u64>,
        next: u64,
        fail_allocate: Option<usize>,
        fail_seal: Option<usize>,
        fail_map: Option<usize>,
        allocations: usize,
        seals: usize,
        maps: usize,
        destroyed: usize,
        fail_replace: bool,
    }
    fn fail(counter: &mut usize, at: Option<usize>) -> bool {
        let current = *counter;
        *counter += 1;
        Some(current) == at
    }
    impl ReadBackend for &mut Model {
        fn sum_page(&self, frame: u64, offset: usize, length: usize) -> u64 {
            let (bytes, sealed) = &self.frames[&frame];
            assert!(sealed);
            bytes[offset..offset + length]
                .iter()
                .map(|&byte| u64::from(byte))
                .sum()
        }
    }
    #[test]
    fn mediated_reads_check_authority_bounds_tail_and_revocation() {
        let mut model = Model::default();
        let mut store = Store::new(&mut model);
        let owner = store.create_filled(0, 4097, 7).unwrap();
        let reader = store.grant(0, owner, 1, Rights::READ).unwrap();
        let no_read = store.grant(0, owner, 2, Rights::MAP).unwrap();
        assert_eq!(store.read_sum(1, reader, 4095, 2), Ok(14));
        assert_eq!(store.read_sum(1, reader, 4097, 0), Ok(0));
        assert_eq!(
            store.read_sum(1, reader, 4097, 1),
            Err(Error::InvalidAddress)
        );
        assert_eq!(
            store.read_sum(1, reader, 4098, 0),
            Err(Error::InvalidAddress)
        );
        assert_eq!(store.read_sum(1, reader, u64::MAX, 1), Err(Error::Overflow));
        assert_eq!(store.read_sum(2, no_read, 0, 1), Err(Error::Denied));
        assert_eq!(store.read_sum(3, owner, 0, 1), Err(Error::StaleHandle));
        store.revoke(0, owner).unwrap();
        let new_owner = store.create_filled(0, 1, 9).unwrap();
        assert_ne!(owner, new_owner);
        assert_eq!(store.read_sum(1, reader, 0, 1), Err(Error::StaleHandle));
        assert_eq!(store.read_sum(0, owner, 0, 1), Err(Error::StaleHandle));
        assert_eq!(store.read_sum(0, new_owner, 0, 1), Ok(9));
    }
    impl Backend for &mut Model {
        fn allocate(&mut self, data: &[u8]) -> Option<u64> {
            if fail(&mut self.allocations, self.fail_allocate) {
                return None;
            }
            self.next += PAGE_SIZE;
            let mut bytes = vec![0; PAGE_SIZE as usize];
            bytes[..data.len()].copy_from_slice(data);
            self.frames.insert(self.next, (bytes, false));
            Some(self.next)
        }
        fn seal(&mut self, frame: u64) -> Result<(), BackendError> {
            if fail(&mut self.seals, self.fail_seal) {
                return Err(BackendError::NoMemory);
            }
            self.frames.get_mut(&frame).unwrap().1 = true;
            Ok(())
        }
        fn destroy(&mut self, frame: u64) {
            assert!(!self.mappings.values().any(|&mapped| mapped == frame));
            assert!(self.frames.remove(&frame).is_some(), "double free");
            self.destroyed += 1;
        }
        fn valid_range(&self, task: usize, address: u64, pages: usize) -> bool {
            task < TASKS && address >= PAGE_SIZE && address + pages as u64 * PAGE_SIZE <= 0x100_0000
        }
        fn map_readonly(
            &mut self,
            task: usize,
            address: u64,
            frame: u64,
        ) -> Result<(), BackendError> {
            assert!(self.frames[&frame].1, "unsealed mapping");
            if fail(&mut self.maps, self.fail_map) {
                return Err(BackendError::NoMemory);
            }
            if self.mappings.contains_key(&(task, address)) {
                return Err(BackendError::Conflict);
            }
            self.mappings.insert((task, address), frame);
            Ok(())
        }
        fn unmap(&mut self, task: usize, address: u64) -> u64 {
            self.mappings.remove(&(task, address)).unwrap()
        }
        fn copy_private(&mut self, original: u64) -> Option<u64> {
            let bytes = self.frames[&original].0.clone();
            self.allocate(&bytes)
        }
        fn replace_private(
            &mut self,
            task: usize,
            address: u64,
            old: u64,
            new: u64,
        ) -> Result<(), BackendError> {
            assert!(self.frames[&old].1 && !self.frames[&new].1);
            if self.fail_replace {
                return Err(BackendError::Conflict);
            }
            assert_eq!(self.mappings.insert((task, address), new), Some(old));
            Ok(())
        }
    }

    #[test]
    fn wire_handles_and_rights_reject_truncation_and_unknown_bits() {
        for generation in [1, 2, u32::MAX] {
            for slot in [0, 15, u16::MAX] {
                let handle = Handle { slot, generation };
                assert_eq!(Handle::decode(handle.encode()), Ok(handle));
                assert_eq!(
                    Handle::decode(handle.encode() | (1 << 48)),
                    Err(Error::StaleHandle)
                );
            }
        }
        assert_eq!(Handle::decode(0), Err(Error::StaleHandle));
        assert_eq!(Handle::decode(u64::MAX), Err(Error::StaleHandle));
        for bits in [0, 128, 1 << 32, u64::MAX] {
            assert_eq!(Rights::from_bits(bits), Err(Error::Denied));
        }
        assert_eq!(Rights::from_bits(79), Ok(Rights::OWNER));
    }

    #[test]
    fn cow_views_are_independent_snapshots_even_with_the_same_capability() {
        let mut model = Model::default();
        let mut store = Store::new(&mut model);
        let owner = store.create_filled(0, 4096, 7).unwrap();
        store.map(0, owner, 0x1000, Rights::COW).unwrap();
        store.map(0, owner, 0x2000, Rights::COW).unwrap();
        store.resolve_cow(0, 0x1000).unwrap();
        let private = store.backend().mappings[&(0, 0x1000)];
        let original = store.backend().mappings[&(0, 0x2000)];
        assert_ne!(private, original);
        store.backend.frames.get_mut(&private).unwrap().0[0] = 99;
        assert_eq!(store.backend().frames[&original].0[0], 7);
        store.resolve_cow(0, 0x2000).unwrap();
        let second = store.backend().mappings[&(0, 0x2000)];
        assert_ne!(private, second);
        assert_eq!(store.backend().frames[&second].0[0], 7);
        store.release_task(0).unwrap();
        assert!(store.backend().frames.is_empty());
    }

    #[test]
    fn cow_copies_only_faulted_page_and_revokes_private_copies() {
        let mut model = Model::default();
        let mut store = Store::new(&mut model);
        let owner = store.create_filled(0, 8192, 0x5a).unwrap();
        let writer = store.grant(0, owner, 1, Rights::OWNER).unwrap();
        let reader = store.grant(0, owner, 2, Rights::READ_MAP).unwrap();
        for (task, handle, mode) in [
            (0, owner, Rights::COW),
            (1, writer, Rights::COW),
            (2, reader, Rights::READ),
        ] {
            store.map(task, handle, 0x1000, mode).unwrap();
        }
        assert_eq!(
            store.map(2, reader, 0x5000, Rights::COW),
            Err(Error::Denied)
        );
        assert_eq!(store.resolve_cow(2, 0x1000), Err(Error::Denied));
        let original = store.backend().mappings[&(0, 0x1000)];
        store.resolve_cow(0, 0x107b).unwrap();
        let private = store.backend().mappings[&(0, 0x1000)];
        assert_ne!(private, original);
        assert_eq!(
            store.backend().frames[&private].0,
            store.backend().frames[&original].0
        );
        store.backend.frames.get_mut(&private).unwrap().0[123] = 0xa7;
        assert_eq!(store.backend().frames[&original].0[123], 0x5a);
        assert_eq!(store.backend().mappings[&(1, 0x1000)], original);
        assert_eq!(
            store.backend().mappings[&(0, 0x2000)],
            store.backend().mappings[&(1, 0x2000)]
        );
        assert_eq!(store.resolve_cow(0, 0x1000), Err(Error::Denied));
        store.resolve_cow(1, 0x1000).unwrap();
        assert_ne!(store.backend().mappings[&(1, 0x1000)], private);
        assert_eq!(store.stats().payload_frames, 4);
        assert_eq!(store.stats().private_frames, 2);
        store.release_task(0).unwrap();
        assert_eq!(store.stats().payload_frames, 3);
        store.revoke(1, writer).unwrap();
        assert_eq!(store.resolve_cow(1, 0x1000), Err(Error::Denied));
        assert_eq!(store.stats().private_frames, 0);
        assert!(store.backend().frames.is_empty());
    }

    #[test]
    fn cow_failure_preserves_source_and_mapping_without_leaks() {
        let mut model = Model::default();
        let mut store = Store::new(&mut model);
        let owner = store.create(0, b"source").unwrap();
        store.map(0, owner, 0x1000, Rights::COW).unwrap();
        let original = store.backend().mappings[&(0, 0x1000)];
        store.backend.fail_allocate = Some(store.backend.allocations);
        assert_eq!(
            store.resolve_cow(0, 0x1000),
            Err(Error::Backend(BackendError::NoMemory))
        );
        store.backend.fail_allocate = None;
        store.backend.fail_replace = true;
        assert_eq!(
            store.resolve_cow(0, 0x1000),
            Err(Error::Backend(BackendError::Conflict))
        );
        assert_eq!(store.backend().mappings[&(0, 0x1000)], original);
        assert_eq!(store.backend().frames.len(), 1);
        assert_eq!(store.stats().private_frames, 0);
        store.backend.fail_replace = false;
        store.resolve_cow(0, 0x1000).unwrap();
        store.unmap(0, owner, 0x1000).unwrap();
        assert_eq!(store.stats().payload_frames, 1);
        assert_eq!(store.stats().private_frames, 0);
        store.release_task(0).unwrap();
        assert!(store.backend().frames.is_empty());
    }

    #[test]
    fn shared_backing_zero_tail_and_sealed_before_mapping() {
        let mut model = Model::default();
        let mut store = Store::new(&mut model);
        let data = [0x5a; 4097];
        let owner = store.create(0, &data).unwrap();
        let reader = store.grant(0, owner, 1, Rights::READ_MAP).unwrap();
        store.map(0, owner, 0x1000, Rights::READ).unwrap();
        store.map(1, reader, 0x2000, Rights::READ).unwrap();
        assert_eq!(store.length(1, reader), Ok(data.len()));
        let backend = store.backend();
        for index in 0..2 {
            assert_eq!(
                backend.mappings[&(0, 0x1000 + index * PAGE_SIZE)],
                backend.mappings[&(1, 0x2000 + index * PAGE_SIZE)]
            );
        }
        let frame = backend.mappings[&(1, 0x3000)];
        assert_eq!(backend.frames[&frame].0[0], 0x5a);
        assert!(backend.frames[&frame].0[1..].iter().all(|&byte| byte == 0));
        assert_eq!(store.stats().payload_frames, 2);
        assert_eq!(store.stats().mapping_pages, 4);
        store.revoke(0, owner).unwrap();
        assert_eq!(store.stats().capabilities, 0);
        assert!(store.backend().frames.is_empty());
        assert_eq!(
            store.map(1, reader, 0x2000, Rights::READ),
            Err(Error::StaleHandle)
        );
    }

    #[test]
    fn task_local_lookup_and_stale_reuse_cannot_reach_other_objects() {
        let mut model = Model::default();
        let mut store = Store::new(&mut model);
        let first = store.create(0, b"first").unwrap();
        assert_eq!(store.length(1, first), Err(Error::StaleHandle));
        let other = store.create(1, b"other task").unwrap();
        assert_eq!(first, other); // Same bits are only authority to each task's own entry.
        assert_eq!(store.length(0, first), Ok(5));
        assert_eq!(store.length(1, first), Ok(10));
        store.release(0, first).unwrap();
        let second = store.create(0, b"second").unwrap();
        assert_eq!(first.slot, second.slot);
        assert_ne!(first.generation, second.generation);
        assert_eq!(store.release(0, first), Err(Error::StaleHandle));
        assert_eq!(store.revoke(0, first), Err(Error::StaleHandle));
        assert_eq!(store.length(TASKS, second), Err(Error::InvalidTask));
        assert_eq!(
            store.length(
                0,
                Handle {
                    slot: u16::MAX,
                    generation: 1
                }
            ),
            Err(Error::StaleHandle)
        );
        assert_eq!(
            store.length(
                0,
                Handle {
                    slot: 0,
                    generation: 0
                }
            ),
            Err(Error::StaleHandle)
        );
        assert_eq!(store.length(1, other), Ok(10));
    }

    #[test]
    fn attenuation_is_transitive_and_write_execute_are_denied() {
        let mut model = Model::default();
        let mut store = Store::new(&mut model);
        let owner = store.create(0, b"immutable").unwrap();
        let sharer = store
            .grant(0, owner, 1, Rights::READ_MAP.union(Rights::SHARE))
            .unwrap();
        let reader = store.grant(1, sharer, 2, Rights::READ).unwrap();
        assert_eq!(store.grant(1, sharer, 2, Rights::OWNER), Err(Error::Denied));
        assert_eq!(store.grant(2, reader, 3, Rights::READ), Err(Error::Denied));
        assert_eq!(store.grant(0, owner, 3, Rights::WRITE), Err(Error::Denied));
        assert_eq!(
            store.map(2, reader, 0x1000, Rights::READ),
            Err(Error::Denied)
        );
        for access in [
            Rights::WRITE,
            Rights::EXECUTE,
            Rights::READ.union(Rights::WRITE),
        ] {
            assert_eq!(store.map(0, owner, 0x1000, access), Err(Error::Denied));
        }
        assert_eq!(store.revoke(1, sharer), Err(Error::Denied));
        store.revoke(0, owner).unwrap();
        assert_eq!(store.length(2, reader), Err(Error::StaleHandle));
    }

    #[test]
    fn release_unmaps_only_its_views_and_last_reference_frees_once() {
        let mut model = Model::default();
        {
            let mut store = Store::new(&mut model);
            let owner = store.create(0, b"data").unwrap();
            let reader = store.grant(0, owner, 1, Rights::READ_MAP).unwrap();
            store.map(0, owner, 0x1000, Rights::READ).unwrap();
            store.map(1, reader, 0x1000, Rights::READ).unwrap();
            assert_eq!(store.unmap(0, owner, 0x2000), Err(Error::NotMapped));
            store.unmap(0, owner, 0x1000).unwrap();
            assert_eq!(store.unmap(0, owner, 0x1000), Err(Error::NotMapped));
            store.release(0, owner).unwrap();
            assert_eq!(store.stats().payload_frames, 1);
            assert_eq!(store.stats().mapping_pages, 1);
            store.release(1, reader).unwrap();
            assert_eq!(store.release(1, reader), Err(Error::StaleHandle));
        }
        assert_eq!(model.destroyed, 1);
        assert!(model.frames.is_empty() && model.mappings.is_empty());
    }

    #[test]
    fn every_partial_create_failure_restores_ownership() {
        for index in 0..3 {
            for seal_failure in [false, true] {
                let mut model = Model::default();
                if seal_failure {
                    model.fail_seal = Some(index);
                } else {
                    model.fail_allocate = Some(index);
                }
                let mut store = Store::new(&mut model);
                assert_eq!(
                    store.create(0, &[9; 8193]),
                    Err(Error::Backend(BackendError::NoMemory))
                );
                assert_eq!(store.stats().objects, 0);
                assert_eq!(store.stats().capabilities, 0);
                assert!(store.backend().frames.is_empty());
                assert_eq!(store.caps[0][0].generation, 1);
                // Failure does not consume a handle or object generation.
                let handle = store.create(0, b"retry").unwrap();
                assert_eq!(
                    handle,
                    Handle {
                        slot: 0,
                        generation: 1
                    }
                );
            }
        }
    }

    #[test]
    fn every_partial_map_failure_rolls_back_but_keeps_payload() {
        for index in 0..3 {
            let mut model = Model {
                fail_map: Some(index),
                ..Model::default()
            };
            let mut store = Store::new(&mut model);
            let owner = store.create(0, &[7; 8193]).unwrap();
            assert_eq!(
                store.map(0, owner, 0x1000, Rights::READ),
                Err(Error::Backend(BackendError::NoMemory))
            );
            assert_eq!(store.stats().mapping_pages, 0);
            assert_eq!(store.stats().payload_frames, 3);
            assert!(store.backend().mappings.is_empty());
            store.map(0, owner, 0x1000, Rights::READ).unwrap();
            store.revoke(0, owner).unwrap();
            assert!(store.backend().frames.is_empty());
        }
    }

    #[test]
    fn overlapping_map_failure_preserves_preexisting_mapping() {
        let mut model = Model::default();
        let mut store = Store::new(&mut model);
        let small = store.create(0, b"keep").unwrap();
        store.map(0, small, 0x2000, Rights::READ).unwrap();
        let original = store.backend().mappings[&(0, 0x2000)];
        let large = store.create(0, &[1; 8193]).unwrap();
        assert_eq!(
            store.map(0, large, 0x1000, Rights::READ),
            Err(Error::Backend(BackendError::Conflict))
        );
        assert_eq!(store.backend().mappings.len(), 1);
        assert_eq!(store.backend().mappings[&(0, 0x2000)], original);
    }

    #[test]
    fn capacity_size_and_address_limits_do_not_change_state() {
        assert_eq!(page_count(usize::MAX), Err(Error::Overflow));
        assert_eq!(page_count(0), Err(Error::InvalidSize));
        assert_eq!(page_count(PAGES * 4096 + 1), Err(Error::InvalidSize));
        let mut model = Model::default();
        let mut store = Store::new(&mut model);
        let owner = store.create(0, b"a").unwrap();
        for _ in 1..CAPS {
            store.grant(0, owner, 0, Rights::READ_MAP).unwrap();
        }
        assert_eq!(store.grant(0, owner, 0, Rights::READ), Err(Error::Capacity));
        assert_eq!(store.create(0, b"full"), Err(Error::Capacity));
        for _ in 1..OBJECTS {
            store.create(1, b"object").unwrap();
        }
        assert_eq!(store.create(1, b"full"), Err(Error::Capacity));
        for address in [0, 1, u64::MAX - 4095, 0x100_0000] {
            assert_eq!(
                store.map(0, owner, address, Rights::READ),
                Err(Error::InvalidAddress)
            );
        }
        for index in 0..VIEWS {
            store
                .map(0, owner, (index as u64 + 1) * PAGE_SIZE, Rights::READ)
                .unwrap();
        }
        assert_eq!(
            store.map(0, owner, 0x100000, Rights::READ),
            Err(Error::Capacity)
        );
        assert_eq!(store.stats().capabilities, CAPS + OBJECTS - 1);
        assert_eq!(store.stats().mapping_pages, VIEWS);
    }

    #[test]
    fn generation_and_reference_overflow_never_wrap() {
        let mut model = Model::default();
        let mut store = Store::new(&mut model);
        store.caps[0][0].generation = u32::MAX;
        store.objects[0].generation = u32::MAX;
        let owner = store.create(0, b"last generation").unwrap();
        store.objects[0].value.as_mut().unwrap().references = u32::MAX;
        assert_eq!(store.grant(0, owner, 1, Rights::READ), Err(Error::Overflow));
        assert_eq!(store.stats().capabilities, 1);
        store.objects[0].value.as_mut().unwrap().references = 1;
        store.release(0, owner).unwrap();
        assert_eq!(store.caps[0][0].generation, 0);
        assert_eq!(store.objects[0].generation, 0);
        let new = store.create(0, b"fresh slot").unwrap();
        assert_eq!(new.slot, 1);
        assert_eq!(store.authority(0, new).unwrap().key.slot, 1);
        assert_eq!(store.release(0, owner), Err(Error::StaleHandle));
    }

    #[test]
    fn repeated_lifecycles_and_drop_return_all_backing() {
        let mut model = Model::default();
        {
            let mut store = Store::new(&mut model);
            for round in 0..1000 {
                let task = round % TASKS;
                let target = (task + 1) % TASKS;
                let owner = store.create(task, &[round as u8; 4097]).unwrap();
                let reader = store.grant(task, owner, target, Rights::READ_MAP).unwrap();
                store.map(task, owner, 0x1000, Rights::READ).unwrap();
                store.map(target, reader, 0x1000, Rights::READ).unwrap();
                if round % 2 == 0 {
                    store.revoke(task, owner).unwrap();
                } else {
                    store.release(task, owner).unwrap();
                    store.release(target, reader).unwrap();
                }
                assert_eq!(store.stats().payload_frames, 0);
                assert_eq!(store.stats().capabilities, 0);
                assert!(store.backend().mappings.is_empty());
            }
            let owner = store.create(0, b"drop with live view").unwrap();
            store.map(0, owner, 0x1000, Rights::READ).unwrap();
        }
        assert_eq!(model.destroyed, 2001);
        assert!(model.frames.is_empty() && model.mappings.is_empty());
    }
}
