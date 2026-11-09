//! Four-level 4 KiB page tables. User roots borrow the kernel's upper half;
//! only their private lower half can be edited. Kernel tables outlive all roots.
use crate::frame::PAGE_SIZE;

const PRESENT: u64 = 1;
const WRITE: u64 = 2;
const USER: u64 = 4;
const HUGE: u64 = 1 << 7;
const NX: u64 = 1 << 63;
const ADDRESS: u64 = 0x000f_ffff_ffff_f000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permissions {
    ReadOnly,
    ReadWrite,
    ReadExecute,
    UserReadOnly,
    UserReadWrite,
    UserReadExecute,
}

impl Permissions {
    fn bits(self) -> u64 {
        PRESENT
            | match self {
                Self::ReadOnly => NX,
                Self::ReadWrite => WRITE | NX,
                Self::ReadExecute => 0,
                Self::UserReadOnly => USER | NX,
                Self::UserReadWrite => USER | WRITE | NX,
                Self::UserReadExecute => USER,
            }
    }
    fn user(self) -> bool {
        self.bits() & USER != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagingError {
    InvalidAddress,
    OutOfFrames,
    AlreadyMapped,
    NotMapped,
    HugePage,
    WrongDomain,
}

/// A table allocation must return a zeroed, private, aligned frame. Free only
/// after invalidation; implementations must not expose unmapped memory as Rust
/// references. M2 uses one CPU, interrupts off, and no shared paging structures.
pub trait TableMemory {
    fn allocate_table(&mut self) -> Option<u64>;
    fn release_table(&mut self, frame: u64);
    fn read(&self, table: u64, index: usize) -> u64;
    fn write(&mut self, table: u64, index: usize, value: u64);
    fn invalidate(&mut self, virtual_address: u64);
}

pub struct PageTables {
    root: u64,
    tables: usize,
    mappings: usize,
    user: bool,
}

fn indices(address: u64) -> Result<[usize; 4], PagingError> {
    let high = address >> 47;
    if !address.is_multiple_of(PAGE_SIZE) || (high != 0 && high != 0x1ffff) {
        return Err(PagingError::InvalidAddress);
    }
    Ok([
        ((address >> 39) & 511) as usize,
        ((address >> 30) & 511) as usize,
        ((address >> 21) & 511) as usize,
        ((address >> 12) & 511) as usize,
    ])
}

impl PageTables {
    pub fn new(memory: &mut impl TableMemory) -> Result<Self, PagingError> {
        let root = memory.allocate_table().ok_or(PagingError::OutOfFrames)?;
        Ok(Self {
            root,
            tables: 1,
            mappings: 0,
            user: false,
        })
    }
    /// The kernel's upper-half tables must outlive this root. Their top-level
    /// entries remain supervisor-only. Edits and teardown require this root to
    /// be inactive, with no cached translations (CR3 reload; no PCID or SMP).
    pub fn new_user(memory: &mut impl TableMemory, kernel: &Self) -> Result<Self, PagingError> {
        assert!(!kernel.user);
        let mut result = Self::new(memory)?;
        result.user = true;
        for index in 256..512 {
            let entry = memory.read(kernel.root, index);
            assert_eq!(entry & USER, 0);
            memory.write(result.root, index, entry);
        }
        Ok(result)
    }
    pub fn destroy_user(self, memory: &mut impl TableMemory) {
        assert!(self.user && self.mappings == 0 && self.tables == 1);
        assert!((0..256).all(|slot| memory.read(self.root, slot) & PRESENT == 0));
        memory.invalidate(0);
        memory.release_table(self.root);
    }
    fn domain(&self, address: u64, permissions: Permissions) -> Result<(), PagingError> {
        if self.user != permissions.user() || (self.user && address >= (1 << 47)) {
            return Err(PagingError::WrongDomain);
        }
        Ok(())
    }
    pub fn root(&self) -> u64 {
        self.root
    }
    pub fn table_frames(&self) -> usize {
        self.tables
    }
    pub fn mappings(&self) -> usize {
        self.mappings
    }

    pub fn map(
        &mut self,
        memory: &mut impl TableMemory,
        virtual_address: u64,
        physical: u64,
        permissions: Permissions,
    ) -> Result<(), PagingError> {
        let indices = indices(virtual_address)?;
        self.domain(virtual_address, permissions)?;
        if physical & !ADDRESS != 0 {
            return Err(PagingError::InvalidAddress);
        }
        let mut table = self.root;
        let mut created = [(0, 0, 0); 3];
        let mut count = 0;
        for &index in &indices[..3] {
            let entry = memory.read(table, index);
            if entry & PRESENT == 0 {
                let Some(child) = memory.allocate_table() else {
                    // Roll back only this call's tables, child before parent.
                    for &(parent, slot, frame) in created[..count].iter().rev() {
                        memory.write(parent, slot, 0);
                        memory.invalidate(virtual_address);
                        memory.release_table(frame);
                        self.tables -= 1;
                    }
                    return Err(PagingError::OutOfFrames);
                };
                memory.write(
                    table,
                    index,
                    child | PRESENT | WRITE | if self.user { USER } else { 0 },
                );
                created[count] = (table, index, child);
                count += 1;
                self.tables += 1;
                table = child;
            } else {
                if entry & HUGE != 0 {
                    return Err(PagingError::HugePage);
                }
                table = entry & ADDRESS;
            }
        }
        if memory.read(table, indices[3]) & PRESENT != 0 {
            return Err(PagingError::AlreadyMapped);
        }
        memory.write(table, indices[3], physical | permissions.bits());
        memory.invalidate(virtual_address);
        self.mappings += 1;
        Ok(())
    }

    pub fn lookup(
        &self,
        memory: &impl TableMemory,
        virtual_address: u64,
    ) -> Result<(u64, Permissions), PagingError> {
        let indices = indices(virtual_address)?;
        let mut table = self.root;
        for &index in &indices[..3] {
            let entry = memory.read(table, index);
            if entry & PRESENT == 0 {
                return Err(PagingError::NotMapped);
            }
            if entry & HUGE != 0 {
                return Err(PagingError::HugePage);
            }
            table = entry & ADDRESS;
        }
        let entry = memory.read(table, indices[3]);
        if entry & PRESENT == 0 {
            return Err(PagingError::NotMapped);
        }
        let permissions = if entry & WRITE != 0 {
            if entry & USER != 0 {
                Permissions::UserReadWrite
            } else {
                Permissions::ReadWrite
            }
        } else if entry & NX == 0 {
            if entry & USER != 0 {
                Permissions::UserReadExecute
            } else {
                Permissions::ReadExecute
            }
        } else {
            if entry & USER != 0 {
                Permissions::UserReadOnly
            } else {
                Permissions::ReadOnly
            }
        };
        Ok((entry & ADDRESS, permissions))
    }

    /// Change an existing leaf without allocating; flush the old permissions.
    /// Caller must enforce alias policy and keep the payload alive.
    pub fn protect(
        &mut self,
        memory: &mut impl TableMemory,
        virtual_address: u64,
        permissions: Permissions,
    ) -> Result<(), PagingError> {
        let (physical, _) = self.lookup(memory, virtual_address)?;
        self.replace(memory, virtual_address, physical, physical, permissions)
    }

    /// Atomically replace an existing leaf. No table allocation and no change
    /// on failure. Caller owns both frames and their alias/lifetime policy.
    pub fn replace(
        &mut self,
        memory: &mut impl TableMemory,
        virtual_address: u64,
        expected: u64,
        physical: u64,
        permissions: Permissions,
    ) -> Result<(), PagingError> {
        let indices = indices(virtual_address)?;
        self.domain(virtual_address, permissions)?;
        if physical & !ADDRESS != 0 {
            return Err(PagingError::InvalidAddress);
        }
        let mut table = self.root;
        for &index in &indices[..3] {
            let entry = memory.read(table, index);
            if entry & PRESENT == 0 {
                return Err(PagingError::NotMapped);
            }
            if entry & HUGE != 0 {
                return Err(PagingError::HugePage);
            }
            table = entry & ADDRESS;
        }
        let entry = memory.read(table, indices[3]);
        if entry & PRESENT == 0 {
            return Err(PagingError::NotMapped);
        }
        if entry & ADDRESS != expected {
            return Err(PagingError::AlreadyMapped);
        }
        memory.write(table, indices[3], physical | permissions.bits());
        memory.invalidate(virtual_address);
        Ok(())
    }

    pub fn unmap(
        &mut self,
        memory: &mut impl TableMemory,
        virtual_address: u64,
    ) -> Result<u64, PagingError> {
        let indices = indices(virtual_address)?;
        if self.user && indices[0] >= 256 {
            return Err(PagingError::WrongDomain);
        }
        let mut path = [self.root; 4];
        for level in 0..3 {
            let entry = memory.read(path[level], indices[level]);
            if entry & PRESENT == 0 {
                return Err(PagingError::NotMapped);
            }
            if entry & HUGE != 0 {
                return Err(PagingError::HugePage);
            }
            path[level + 1] = entry & ADDRESS;
        }
        let entry = memory.read(path[3], indices[3]);
        if entry & PRESENT == 0 {
            return Err(PagingError::NotMapped);
        }
        memory.write(path[3], indices[3], 0);
        memory.invalidate(virtual_address);
        self.mappings -= 1;
        for level in (1..4).rev() {
            if (0..512).any(|slot| memory.read(path[level], slot) & PRESENT != 0) {
                break;
            }
            memory.write(path[level - 1], indices[level - 1], 0);
            // Also invalidate paging-structure caches after clearing the parent.
            memory.invalidate(virtual_address);
            memory.release_table(path[level]);
            self.tables -= 1;
        }
        Ok(entry & ADDRESS)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::{collections::BTreeMap, vec::Vec};

    struct Model {
        tables: BTreeMap<u64, [u64; 512]>,
        limit: usize,
        next: u64,
        events: Vec<&'static str>,
    }
    impl Model {
        fn new(limit: usize) -> Self {
            Self {
                tables: BTreeMap::new(),
                limit,
                next: 4096,
                events: Vec::new(),
            }
        }
    }
    impl TableMemory for Model {
        fn allocate_table(&mut self) -> Option<u64> {
            if self.tables.len() >= self.limit {
                return None;
            }
            let frame = self.next;
            self.next += 4096;
            self.tables.insert(frame, [0; 512]);
            Some(frame)
        }
        fn release_table(&mut self, frame: u64) {
            assert_eq!(self.events.last(), Some(&"invalidate"));
            self.events.push("release");
            assert!(self.tables.remove(&frame).is_some());
        }
        fn read(&self, table: u64, index: usize) -> u64 {
            self.tables[&table][index]
        }
        fn write(&mut self, table: u64, index: usize, value: u64) {
            self.tables.get_mut(&table).unwrap()[index] = value;
            self.events.push("write");
        }
        fn invalidate(&mut self, _: u64) {
            self.events.push("invalidate");
        }
    }

    #[test]
    fn user_roots_isolate_lower_half_and_borrow_only_supervisor_kernel() {
        let mut memory = Model::new(32);
        let mut kernel = PageTables::new(&mut memory).unwrap();
        let high = 0xffff_8000_0000_1000;
        kernel
            .map(&mut memory, high, 0x90000, Permissions::ReadWrite)
            .unwrap();
        let kernel_tables = kernel.table_frames();
        let mut first = PageTables::new_user(&mut memory, &kernel).unwrap();
        let mut second = PageTables::new_user(&mut memory, &kernel).unwrap();
        first
            .map(&mut memory, 0x400000, 0xa0000, Permissions::UserReadOnly)
            .unwrap();
        second
            .map(&mut memory, 0x400000, 0xb0000, Permissions::UserReadExecute)
            .unwrap();
        assert_eq!(
            kernel.lookup(&memory, 0x400000),
            Err(PagingError::NotMapped)
        );
        assert_eq!(
            first.lookup(&memory, high),
            Ok((0x90000, Permissions::ReadWrite))
        );
        for index in indices(0x400000).unwrap().into_iter().take(1) {
            assert_ne!(memory.read(first.root, index) & USER, 0);
        }
        assert_eq!(
            first.protect(&mut memory, high, Permissions::UserReadOnly),
            Err(PagingError::WrongDomain)
        );
        assert_eq!(
            first.unmap(&mut memory, high),
            Err(PagingError::WrongDomain)
        );
        assert_eq!(
            kernel.map(&mut memory, 0x1000, 0xc0000, Permissions::UserReadOnly),
            Err(PagingError::WrongDomain)
        );
        assert_eq!(
            first.replace(
                &mut memory,
                0x400000,
                0xb0000,
                0xc0000,
                Permissions::UserReadWrite
            ),
            Err(PagingError::AlreadyMapped)
        );
        first
            .replace(
                &mut memory,
                0x400000,
                0xa0000,
                0xc0000,
                Permissions::UserReadWrite,
            )
            .unwrap();
        assert_eq!(
            second.lookup(&memory, 0x400000),
            Ok((0xb0000, Permissions::UserReadExecute))
        );
        assert_eq!(first.unmap(&mut memory, 0x400000), Ok(0xc0000));
        second.unmap(&mut memory, 0x400000).unwrap();
        first.destroy_user(&mut memory);
        second.destroy_user(&mut memory);
        assert_eq!(memory.tables.len(), kernel_tables);
        assert_eq!(
            kernel.lookup(&memory, high),
            Ok((0x90000, Permissions::ReadWrite))
        );
    }

    #[test]
    fn protect_flushes_permissions_without_allocating_or_changing_backing() {
        let mut memory = Model::new(4);
        let mut pages = PageTables::new(&mut memory).unwrap();
        assert_eq!(
            pages.protect(&mut memory, 0x1000, Permissions::ReadOnly),
            Err(PagingError::NotMapped)
        );
        pages
            .map(&mut memory, 0x1000, 0x9000, Permissions::ReadWrite)
            .unwrap();
        for permissions in [
            Permissions::ReadOnly,
            Permissions::ReadExecute,
            Permissions::ReadWrite,
        ] {
            pages.protect(&mut memory, 0x1000, permissions).unwrap();
            assert_eq!(pages.lookup(&memory, 0x1000), Ok((0x9000, permissions)));
            assert_eq!(memory.events.last(), Some(&"invalidate"));
            assert_eq!(pages.table_frames(), 4);
            assert_eq!(pages.mappings(), 1);
        }
        assert_eq!(
            pages.protect(&mut memory, 1, Permissions::ReadOnly),
            Err(PagingError::InvalidAddress)
        );
    }

    #[test]
    fn map_permissions_duplicate_unmap_and_prune() {
        let mut memory = Model::new(16);
        let mut pages = PageTables::new(&mut memory).unwrap();
        pages
            .map(
                &mut memory,
                0xffff_ff00_0000_0000,
                0x8000,
                Permissions::ReadOnly,
            )
            .unwrap();
        assert_eq!(
            pages.lookup(&memory, 0xffff_ff00_0000_0000),
            Ok((0x8000, Permissions::ReadOnly))
        );
        assert_eq!(
            pages.map(
                &mut memory,
                0xffff_ff00_0000_0000,
                0x9000,
                Permissions::ReadWrite
            ),
            Err(PagingError::AlreadyMapped)
        );
        assert_eq!(pages.unmap(&mut memory, 0xffff_ff00_0000_0000), Ok(0x8000));
        assert_eq!(
            pages.lookup(&memory, 0xffff_ff00_0000_0000),
            Err(PagingError::NotMapped)
        );
        assert_eq!(
            pages.unmap(&mut memory, 0xffff_ff00_0000_0000),
            Err(PagingError::NotMapped)
        );
        assert_eq!(pages.table_frames(), 1);
        assert_eq!(pages.mappings(), 0);
        assert_eq!(memory.tables.len(), 1);
    }

    #[test]
    fn partial_allocation_failure_rolls_back_all_new_tables() {
        for limit in 1..4 {
            let mut memory = Model::new(limit);
            let mut pages = PageTables::new(&mut memory).unwrap();
            assert_eq!(
                pages.map(&mut memory, 0, 0x8000, Permissions::ReadWrite),
                Err(PagingError::OutOfFrames)
            );
            assert_eq!(pages.table_frames(), 1);
            assert_eq!(memory.tables.len(), 1);
            assert!(memory.tables[&pages.root()].iter().all(|&entry| entry == 0));
        }
    }

    #[test]
    fn adjacent_mapping_survives_pruning_and_failed_expansion() {
        let mut memory = Model::new(4);
        let mut pages = PageTables::new(&mut memory).unwrap();
        pages
            .map(&mut memory, 0x1000, 0x8000, Permissions::ReadWrite)
            .unwrap();
        pages
            .map(&mut memory, 0x2000, 0x9000, Permissions::ReadExecute)
            .unwrap();
        assert_eq!(
            pages.map(&mut memory, 0x20_0000, 0xa000, Permissions::ReadOnly),
            Err(PagingError::OutOfFrames)
        );
        pages.unmap(&mut memory, 0x1000).unwrap();
        assert_eq!(
            pages.lookup(&memory, 0x2000),
            Ok((0x9000, Permissions::ReadExecute))
        );
        assert_eq!(pages.table_frames(), 4);
    }

    #[test]
    fn malformed_addresses_do_not_allocate_tables() {
        let mut memory = Model::new(16);
        let mut pages = PageTables::new(&mut memory).unwrap();
        for (virt, physical) in [
            (1, 4096),
            (0x8000_0000_0000, 4096),
            (0xffff_0000_0000_0000, 4096),
            (0, 4097),
            (0, 1 << 52),
        ] {
            assert_eq!(
                pages.map(&mut memory, virt, physical, Permissions::ReadOnly),
                Err(PagingError::InvalidAddress)
            );
        }
        assert_eq!(pages.table_frames(), 1);
    }
}
