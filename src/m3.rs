//! Kernel integration, not ring-3 isolation: logical task tables use distinct
//! reserved VA windows in the current supervisor address space. M4 replaces
//! this backend with task address spaces and a trusted syscall caller context.
use crate::{
    faults::{self, Probe},
    m2::PhysicalMemory,
    serial,
};
use shared_first_os::{
    frame::PAGE_SIZE,
    object::{Backend, BackendError, Error, Rights, Store, TASKS},
    paging::{PageTables, PagingError, Permissions, TableMemory},
};

const WINDOW: u64 = 0x20_0000;
fn task_base(task: usize) -> u64 {
    0xffff_fd00_0000_0000 + task as u64 * WINDOW
}

struct KernelMemory<'a> {
    memory: &'a mut PhysicalMemory,
    pages: &'a mut PageTables,
    allocation_budget: Option<usize>, // Deterministic allocation-failure injection.
}
impl Backend for KernelMemory<'_> {
    fn allocate(&mut self, data: &[u8]) -> Option<u64> {
        assert!(data.len() <= PAGE_SIZE as usize);
        if let Some(budget) = &mut self.allocation_budget {
            if *budget == 0 {
                return None;
            }
            *budget -= 1;
        }
        let frame = self.memory.allocate_table()?; // Same zeroed-frame primitive.
        let address = (self.memory.hhdm + frame) as *mut u8;
        for (index, &byte) in data.iter().enumerate() {
            // SAFETY: exclusive allocated, zeroed frame with a private RW/NX
            // HHDM alias. No object/view exposes it until initialization + seal.
            unsafe {
                address.add(index).write_volatile(byte);
            }
        }
        Some(frame)
    }
    fn seal(&mut self, frame: u64) -> Result<(), BackendError> {
        // Newly allocated payload has exactly one alias, in the HHDM. All later
        // object views are R/NX. No page-table or stack frame enters this API.
        self.pages
            .protect(self.memory, self.memory.hhdm + frame, Permissions::ReadOnly)
            .expect("seal payload HHDM alias");
        Ok(())
    }
    fn destroy(&mut self, frame: u64) {
        self.pages
            .protect(
                self.memory,
                self.memory.hhdm + frame,
                Permissions::ReadWrite,
            )
            .expect("restore private payload alias");
        for index in 0..512 {
            // SAFETY: Store has removed every view and completed local TLB
            // invalidation before the final release; sole CPU owns this frame.
            unsafe {
                self.memory.pointer(frame, index).write_volatile(0);
            }
        }
        self.memory
            .frames
            .release(frame)
            .expect("object frame release");
    }
    fn valid_range(&self, task: usize, address: u64, pages: usize) -> bool {
        task < TASKS
            && address >= task_base(task)
            && address + pages as u64 * PAGE_SIZE <= task_base(task) + WINDOW
    }
    fn map_readonly(&mut self, _task: usize, address: u64, frame: u64) -> Result<(), BackendError> {
        self.pages
            .map(self.memory, address, frame, Permissions::ReadOnly)
            .map_err(|error| match error {
                PagingError::OutOfFrames => BackendError::NoMemory,
                PagingError::AlreadyMapped => BackendError::Conflict,
                _ => panic!("object backend mapping invariant: {error:?}"),
            })
    }
    fn unmap(&mut self, _task: usize, address: u64) -> u64 {
        self.pages
            .unmap(self.memory, address)
            .expect("tracked object view")
    }
}

fn probe(address: u64, kind: Probe) {
    // SAFETY: callers below use only retained test payloads (RET at byte 0 for
    // NX) or freshly unmapped test windows. No Rust references alias payloads.
    unsafe {
        faults::expect_named(address, kind, "M3FAULT");
    }
}
fn read(address: u64) -> u8 {
    // SAFETY: private test helper, called only for a live, retained object view.
    unsafe { (address as *const u8).read_volatile() }
}

pub fn run(memory: &mut PhysicalMemory, pages: &mut PageTables) {
    let before = memory.frames.allocated();
    let tables_before = pages.table_frames();
    let mappings_before = pages.mappings();
    let faults_before = faults::count();
    let mut data = [0xa5; 4097];
    data[0] = 0xc3; // RET ensures the NX probe returns safely if NX is broken.

    {
        let mut store = Store::new(KernelMemory {
            memory,
            pages,
            allocation_budget: Some(1),
        });
        assert_eq!(
            store.create(0, &data),
            Err(Error::Backend(BackendError::NoMemory))
        );
        assert_eq!(store.stats().payload_frames, 0);
        assert_eq!(store.stats().capabilities, 0);
    }
    assert_eq!(memory.frames.allocated(), before);
    {
        let mut store = Store::new(KernelMemory {
            memory,
            pages,
            allocation_budget: None,
        });
        let owner = store.create(0, &data).unwrap();
        assert_eq!(store.length(2, owner), Err(Error::StaleHandle));
        let reader = store.grant(0, owner, 1, Rights::READ_MAP).unwrap();
        assert_eq!(
            store.grant(1, reader, 2, Rights::READ_MAP),
            Err(Error::Denied)
        );
        assert_eq!(store.grant(0, owner, 1, Rights::WRITE), Err(Error::Denied));
        assert_eq!(store.revoke(1, reader), Err(Error::Denied));
        let owner_va = task_base(0) + PAGE_SIZE;
        let reader_va = task_base(1) + PAGE_SIZE;
        for access in [Rights::WRITE, Rights::EXECUTE] {
            assert_eq!(store.map(0, owner, owner_va, access), Err(Error::Denied));
        }
        assert_eq!(
            store.map(0, owner, reader_va, Rights::READ),
            Err(Error::InvalidAddress)
        );
        store.map(0, owner, owner_va, Rights::READ).unwrap();
        let after_first = store.backend().memory.frames.allocated();
        let tables_first = store.backend().pages.table_frames();
        store.map(1, reader, reader_va, Rights::READ).unwrap();
        let stats = store.stats();
        let backend = store.backend();
        assert_eq!(stats.payload_frames, 2);
        assert_eq!(stats.mapping_pages, 4);
        assert_eq!(
            backend.memory.frames.allocated() - after_first,
            backend.pages.table_frames() - tables_first
        );
        serial::print(format_args!(
            "M3OBJECT length={} payload_frames={} capabilities={} mapping_pages={} metadata_bytes={} allocated={} table_frames={}\n",
            store.length(0, owner).unwrap(),
            stats.payload_frames,
            stats.capabilities,
            stats.mapping_pages,
            stats.metadata_bytes,
            backend.memory.frames.allocated(),
            backend.pages.table_frames()
        ));
        for index in 0..2 {
            let owner_page = owner_va + index * PAGE_SIZE;
            let reader_page = reader_va + index * PAGE_SIZE;
            let (frame, rights) = backend.pages.lookup(backend.memory, owner_page).unwrap();
            assert_eq!(rights, Permissions::ReadOnly);
            assert_eq!(
                backend.pages.lookup(backend.memory, reader_page),
                Ok((frame, rights))
            );
            let alias = backend.memory.hhdm + frame;
            assert_eq!(
                backend.pages.lookup(backend.memory, alias),
                Ok((frame, rights))
            );
            serial::print(format_args!(
                "M3PAGE index={} physical={:#x} owner_va={:#x} reader_va={:#x} hhdm={:#x} rights=ro_nx\n",
                index, frame, owner_page, reader_page, alias
            ));
            probe(alias, Probe::WriteReadOnly);
        }
        for index in 0..8192 {
            let expected = data.get(index).copied().unwrap_or(0);
            assert_eq!(read(owner_va + index as u64), expected);
            assert_eq!(read(reader_va + index as u64), expected);
        }
        probe(owner_va, Probe::WriteReadOnly);
        probe(reader_va, Probe::WriteReadOnly);
        probe(reader_va, Probe::ExecuteNx);
        store.revoke(0, owner).unwrap();
        probe(owner_va, Probe::ReadUnmapped);
        probe(reader_va, Probe::ReadUnmapped);
        assert_eq!(store.release(0, owner), Err(Error::StaleHandle));
        assert_eq!(
            store.map(1, reader, reader_va, Rights::READ),
            Err(Error::StaleHandle)
        );
        assert_eq!(store.stats().payload_frames, 0);
        let next = store.create(0, b"next").unwrap();
        assert_eq!(next.slot, owner.slot);
        assert_ne!(next.generation, owner.generation);
        let next_reader = store.grant(0, next, 1, Rights::READ_MAP).unwrap();
        store.map(0, next, owner_va, Rights::READ).unwrap();
        store.map(1, next_reader, reader_va, Rights::READ).unwrap();
        assert_eq!(read(owner_va), b'n'); // VA and backing reuse, with TLB invalidation.
        store.release(0, next).unwrap();
        probe(owner_va, Probe::ReadUnmapped);
        assert_eq!(read(reader_va), b'n');
        assert_eq!(store.stats().payload_frames, 1);
        store.release(1, next_reader).unwrap();
        probe(reader_va, Probe::ReadUnmapped);
        assert_eq!(store.stats().objects, 0);
        assert_eq!(store.stats().capabilities, 0);

        // Real PTE conflict on page 2 must roll back page 1 only.
        let small = store.create(0, b"keep").unwrap();
        store
            .map(0, small, owner_va + PAGE_SIZE, Rights::READ)
            .unwrap();
        let large = store.create(0, &data).unwrap();
        let allocated = store.backend().memory.frames.allocated();
        assert_eq!(
            store.map(0, large, owner_va, Rights::READ),
            Err(Error::Backend(BackendError::Conflict))
        );
        assert_eq!(
            store
                .backend()
                .pages
                .lookup(store.backend().memory, owner_va),
            Err(PagingError::NotMapped)
        );
        assert_eq!(read(owner_va + PAGE_SIZE), b'k');
        assert_eq!(store.backend().memory.frames.allocated(), allocated);
        assert_eq!(store.stats().mapping_pages, 1);
        // Drop exercises teardown with a live mapping and two live objects.
    }
    assert_eq!(memory.frames.allocated(), before);
    assert_eq!(pages.table_frames(), tables_before);
    assert_eq!(pages.mappings(), mappings_before);
    let faults = faults::count() - faults_before;
    assert_eq!(faults, 9);
    serial::print(format_args!(
        "M3FRAMES before={} after={} tables_before={} tables_after={} mappings_before={} mappings_after={} payload_frames=0 capabilities=0 objects=0\n",
        before,
        memory.frames.allocated(),
        tables_before,
        pages.table_frames(),
        mappings_before,
        pages.mappings()
    ));
    serial::print(format_args!(
        "M3CHECK shared_frames=1 zero_tail=1 rights=1 stale=1 revoke=1 release=1 create_rollback=1 map_rollback=1 drop_cleanup=1 hhdm_readonly=1 faults={}\n",
        faults
    ));
    serial::print(format_args!("M3 PASS\n"));
}
