//! Single-CPU M2 bring-up. All mutable global storage is claimed exactly once
//! with interrupts disabled; no bootloader frames are reclaimed.
use crate::{
    boot,
    faults::{self, Probe},
    machine, serial,
};
use core::{mem::MaybeUninit, ptr};
use shared_first_os::{
    frame::{FrameAllocator, PAGE_SIZE},
    memory::Region,
    paging::{PageTables, PagingError, Permissions, TableMemory},
};

const BITMAP_WORDS: usize = 4096; // Explicit 1 GiB physical allocation limit.
const STACK_BOTTOM: u64 = 0xffff_fe80_0000_1000;
const STACK_PAGES: usize = 16;
const STACK_TOP: u64 = STACK_BOTTOM + STACK_PAGES as u64 * PAGE_SIZE;
const TEST_VIRTUAL: u64 = 0xffff_ff00_0000_0000;
static mut ELIGIBLE: [u64; BITMAP_WORDS] = [0; BITMAP_WORDS];
static mut USED: [u64; BITMAP_WORDS] = [0; BITMAP_WORDS];
static mut EXHAUSTED: [u64; BITMAP_WORDS] = [0; BITMAP_WORDS];
static mut REGIONS: [Region; 4096] = [Region {
    base: 0,
    length: 0,
    kind: 0,
}; 4096];
static mut STATE: MaybeUninit<State> = MaybeUninit::uninit();

unsafe extern "C" {
    static __kernel_start: u8;
    static __kernel_end: u8;
    static __text_start: u8;
    static __text_end: u8;
    static __rodata_start: u8;
    static __rodata_end: u8;
}

pub(crate) struct PhysicalMemory {
    pub(crate) frames: FrameAllocator<'static>,
    pub(crate) hhdm: u64,
    active: bool,
}

impl PhysicalMemory {
    pub(crate) fn pointer(&self, frame: u64, index: usize) -> *mut u64 {
        assert!(self.frames.is_allocated(frame) && index < 512);
        (self.hhdm + frame + index as u64 * 8) as *mut u64
    }
}

impl TableMemory for PhysicalMemory {
    fn allocate_table(&mut self) -> Option<u64> {
        let frame = self.frames.allocate()?;
        for index in 0..512 {
            // SAFETY: the allocator returns an exclusive usable frame; Limine
            // and our replacement tree both map every such frame in the HHDM.
            unsafe {
                self.pointer(frame, index).write_volatile(0);
            }
        }
        Some(frame)
    }
    fn release_table(&mut self, frame: u64) {
        self.frames
            .release(frame)
            .expect("release page-table frame");
    }
    fn read(&self, table: u64, index: usize) -> u64 {
        // SAFETY: pointer validates allocation and bounds; table frames are
        // initialized, retained, and accessed only by this CPU. Volatile observes
        // hardware-maintained accessed/dirty bits without creating references.
        unsafe { self.pointer(table, index).read_volatile() }
    }
    fn write(&mut self, table: u64, index: usize, value: u64) {
        // SAFETY: same exclusive-frame invariant as read. PageTables invalidates
        // translations before releasing any table or returning an unmapped frame.
        unsafe {
            self.pointer(table, index).write_volatile(value);
        }
    }
    fn invalidate(&mut self, address: u64) {
        if self.active {
            machine::invalidate_page(address);
        }
    }
}

struct State {
    memory: PhysicalMemory,
    pages: PageTables,
    physical_base: u64,
    virtual_base: u64,
    region_count: usize,
}

fn kernel_permissions(address: u64) -> Permissions {
    if (ptr::addr_of!(__text_start) as u64..ptr::addr_of!(__text_end) as u64).contains(&address) {
        Permissions::ReadExecute
    } else if (ptr::addr_of!(__rodata_start) as u64..ptr::addr_of!(__rodata_end) as u64)
        .contains(&address)
    {
        Permissions::ReadOnly
    } else {
        Permissions::ReadWrite
    }
}

pub fn initialize(map: &boot::MemoryMap) -> ! {
    faults::init();
    let (hhdm, physical_base, virtual_base) = boot::address_info();
    let kernel_start = ptr::addr_of!(__kernel_start) as u64;
    let kernel_end = ptr::addr_of!(__kernel_end) as u64;
    assert_eq!(virtual_base, kernel_start);
    assert!(hhdm.is_multiple_of(PAGE_SIZE) && physical_base.is_multiple_of(PAGE_SIZE));
    // SAFETY: initialize is entered once on the boot CPU. These disjoint static
    // arrays have no other references; their pages belong to the reserved kernel
    // image. Borrowing the arrays does not borrow the separate STATE storage.
    let (eligible, used, regions) = unsafe {
        (
            &mut *ptr::addr_of_mut!(ELIGIBLE),
            &mut *ptr::addr_of_mut!(USED),
            &mut *ptr::addr_of_mut!(REGIONS),
        )
    };
    for (index, destination) in regions[..map.len()].iter_mut().enumerate() {
        *destination = map.region(index).expect("memory map snapshot");
    }
    let regions = &regions[..map.len()];
    let frames =
        FrameAllocator::new(eligible, used, regions).expect("frame allocator initialization");
    serial::print(format_args!(
        "M2START total_frames={} bitmap_bytes={} limit_bytes=1073741824\n",
        frames.total(),
        frames.metadata_bytes()
    ));
    let mut memory = PhysicalMemory {
        frames,
        hhdm,
        active: false,
    };
    let mut pages = PageTables::new(&mut memory).expect("root page table");
    for region in regions
        .iter()
        .filter(|region| matches!(region.kind, 0 | 5 | 6))
    {
        assert!(region.base.is_multiple_of(PAGE_SIZE) && region.length.is_multiple_of(PAGE_SIZE));
        for physical in
            (region.base.max(PAGE_SIZE)..region.base + region.length).step_by(PAGE_SIZE as usize)
        {
            let in_kernel =
                physical >= physical_base && physical - physical_base < kernel_end - kernel_start;
            let permissions = if in_kernel
                && kernel_permissions(virtual_base + physical - physical_base)
                    != Permissions::ReadWrite
            {
                Permissions::ReadOnly // No writable alias of RX text or RO data.
            } else {
                Permissions::ReadWrite
            };
            pages
                .map(
                    &mut memory,
                    hhdm.checked_add(physical).expect("HHDM overflow"),
                    physical,
                    permissions,
                )
                .expect("HHDM mapping");
        }
    }
    for virtual_address in (kernel_start..kernel_end).step_by(PAGE_SIZE as usize) {
        pages
            .map(
                &mut memory,
                virtual_address,
                physical_base + virtual_address - virtual_base,
                kernel_permissions(virtual_address),
            )
            .expect("kernel image mapping");
    }
    for index in 0..STACK_PAGES {
        let frame = memory.frames.allocate().expect("kernel stack frame");
        // SAFETY: exclusive allocated frame, fully mapped in HHDM. Zero before
        // exposing it as a stack; no Rust references to its contents exist.
        for slot in 0..512 {
            // SAFETY: the exclusive allocated stack frame is HHDM-mapped.
            unsafe {
                memory.pointer(frame, slot).write_volatile(0);
            }
        }
        pages
            .map(
                &mut memory,
                STACK_BOTTOM + index as u64 * PAGE_SIZE,
                frame,
                Permissions::ReadWrite,
            )
            .expect("kernel stack mapping");
    }
    assert_eq!(
        pages.lookup(&memory, STACK_BOTTOM - PAGE_SIZE),
        Err(PagingError::NotMapped)
    );
    assert_eq!(
        pages.lookup(&memory, STACK_TOP),
        Err(PagingError::NotMapped)
    );
    let root = pages.root();
    // SAFETY: new tree maps all current kernel segments, all bootloader-reclaimable
    // memory (including the live loader stack/GDT), and our IDT and static state.
    // Old page tables remain reserved. No AP or interrupt can race the CR3 switch.
    unsafe {
        machine::activate_paging(root);
    }
    memory.active = true;
    serial::print(format_args!(
        "M2PAGING root={:#x} table_frames={} mappings={} wp=1 nx=1\n",
        root,
        pages.table_frames(),
        pages.mappings()
    ));
    // SAFETY: STATE is initialized once, with no aliases. All objects point only
    // to persistent static storage or physical frames, never into the old stack.
    unsafe {
        ptr::addr_of_mut!(STATE).write(MaybeUninit::new(State {
            memory,
            pages,
            physical_base,
            virtual_base,
            region_count: map.len(),
        }));
    }
    // SAFETY: the 64 KiB stack is exclusively owned, zeroed, RW/NX and bounded by
    // unmapped pages. There are no live references into the old stack after this.
    unsafe {
        machine::switch_stack(STACK_TOP, run_tests);
    }
}

#[inline(never)]
fn code_probe() -> u64 {
    core::hint::black_box(0xcafe)
}

extern "C" fn run_tests() -> ! {
    // SAFETY: STATE was initialized before the non-returning stack transition;
    // this is its only mutable borrow, on the only active CPU.
    let state = unsafe { (&mut *ptr::addr_of_mut!(STATE)).assume_init_mut() };
    let State {
        memory,
        pages,
        physical_base,
        virtual_base,
        region_count,
    } = state;
    let stack = machine::stack_pointer();
    assert!((STACK_BOTTOM..STACK_TOP).contains(&stack));
    serial::print(format_args!(
        "M2STACK rsp={:#x} bottom={:#x} top={:#x} guards=2\n",
        stack, STACK_BOTTOM, STACK_TOP
    ));
    // SAFETY: probe from a healthy stack, targeting only unmapped guard pages.
    unsafe {
        faults::expect(STACK_BOTTOM - PAGE_SIZE, Probe::ReadUnmapped);
        faults::expect(STACK_TOP, Probe::ReadUnmapped);
    }
    assert_eq!(code_probe(), 0xcafe);
    let code = code_probe as *const () as u64;
    let code_physical = *physical_base + code - *virtual_base;
    // SAFETY: this isolated function is not used after these write probes. A
    // failed protection assertion aborts the boot; successful probes change no bytes.
    unsafe {
        faults::expect(code, Probe::WriteReadOnly);
        faults::expect(memory.hhdm + code_physical, Probe::WriteReadOnly);
    }
    // Audit every kernel-image page and its HHDM alias, not only the probe page.
    for address in (*virtual_base..ptr::addr_of!(__kernel_end) as u64).step_by(PAGE_SIZE as usize) {
        let expected = kernel_permissions(address);
        let physical = *physical_base + address - *virtual_base;
        assert_eq!(pages.lookup(memory, address), Ok((physical, expected)));
        let alias = if expected == Permissions::ReadWrite {
            Permissions::ReadWrite
        } else {
            Permissions::ReadOnly
        };
        assert_eq!(
            pages.lookup(memory, memory.hhdm + physical),
            Ok((physical, alias))
        );
    }
    let baseline = memory.frames.allocated();
    // SAFETY: the snapshot was initialized once and is immutable after startup.
    let regions = unsafe {
        core::slice::from_raw_parts(ptr::addr_of!(REGIONS).cast::<Region>(), *region_count)
    };
    for index in 0..BITMAP_WORDS * 64 {
        let address = index as u64 * PAGE_SIZE;
        if memory.frames.is_allocated(address) {
            assert_usable_frame(address, regions);
        }
    }
    let baseline_tables = pages.table_frames();
    let baseline_mappings = pages.mappings();
    let first = memory.frames.allocate().expect("first payload");
    let second = memory.frames.allocate().expect("second payload");
    assert_ne!(first, second);
    pages
        .map(memory, TEST_VIRTUAL, first, Permissions::ReadWrite)
        .unwrap();
    // SAFETY: TEST_VIRTUAL and the two HHDM aliases refer to owned payload frames;
    // raw volatile accesses deliberately test physical identity without Rust aliases.
    unsafe {
        (TEST_VIRTUAL as *mut u64).write_volatile(0x1122334455667788);
        assert_eq!(memory.pointer(first, 0).read_volatile(), 0x1122334455667788);
        memory.pointer(second, 0).write_volatile(0xaabbccddeeff0011);
    }
    assert_eq!(pages.unmap(memory, TEST_VIRTUAL), Ok(first));
    // SAFETY: no mapping remains and INVLPG has completed on this sole CPU.
    unsafe {
        faults::expect(TEST_VIRTUAL, Probe::ReadUnmapped);
    }
    pages
        .map(memory, TEST_VIRTUAL, second, Permissions::ReadWrite)
        .unwrap();
    // SAFETY: this newly mapped VA must now address the second frame; stale TLB
    // state would return the first frame's distinct marker and fail the assertion.
    unsafe {
        assert_eq!(
            (TEST_VIRTUAL as *const u64).read_volatile(),
            0xaabbccddeeff0011
        );
    }
    pages.unmap(memory, TEST_VIRTUAL).unwrap();
    pages
        .map(memory, TEST_VIRTUAL, first, Permissions::ReadOnly)
        .unwrap();
    // SAFETY: controlled RO mapping of a retained payload frame.
    unsafe {
        faults::expect(TEST_VIRTUAL, Probe::WriteReadOnly);
    }
    pages.unmap(memory, TEST_VIRTUAL).unwrap();
    pages
        .map(memory, TEST_VIRTUAL, second, Permissions::ReadWrite)
        .unwrap();
    // SAFETY: write a RET byte into our payload, then attempt instruction fetch
    // through its NX mapping. RET makes even a missing-NX failure return safely.
    unsafe {
        (TEST_VIRTUAL as *mut u8).write_volatile(0xc3);
        faults::expect(TEST_VIRTUAL, Probe::ExecuteNx);
    }
    pages.unmap(memory, TEST_VIRTUAL).unwrap();
    memory.frames.release(first).unwrap();
    memory.frames.release(second).unwrap();
    assert_eq!(memory.frames.allocated(), baseline);
    assert_eq!(pages.table_frames(), baseline_tables);
    assert_eq!(pages.mappings(), baseline_mappings);

    // Independent bitmap catches duplicates during real allocator exhaustion.
    // SAFETY: this separate zero-initialized test bitmap is exclusively claimed
    // here; it is kernel-image storage, never eligible for frame allocation.
    let exhausted = unsafe { &mut *ptr::addr_of_mut!(EXHAUSTED) };
    let expected_free = memory.frames.free();
    let mut acquired = 0;
    while let Some(frame) = memory.frames.allocate() {
        assert!(memory.frames.is_eligible(frame));
        assert_usable_frame(frame, regions);
        let index = (frame / PAGE_SIZE) as usize;
        assert_eq!(exhausted[index / 64] & (1 << (index % 64)), 0);
        exhausted[index / 64] |= 1 << (index % 64);
        acquired += 1;
    }
    assert_eq!(acquired, expected_free);
    assert_eq!(memory.frames.free(), 0);
    for (word_index, word) in exhausted.iter_mut().enumerate() {
        while *word != 0 {
            let bit = word.trailing_zeros() as usize;
            let frame = (word_index * 64 + bit) as u64 * PAGE_SIZE;
            memory.frames.release(frame).unwrap();
            *word &= !(1 << bit);
        }
    }
    assert_eq!(memory.frames.allocated(), baseline);
    assert_eq!(memory.frames.free(), expected_free);
    assert_eq!(baseline, pages.table_frames() + STACK_PAGES);
    assert_eq!(faults::count(), 7);
    serial::print(format_args!(
        "M2FRAMES total={} allocated={} free={} table_frames={} stack_frames={} payload_frames=0 bitmap_bytes={} scratch_bytes={} mappings={}\n",
        memory.frames.total(),
        baseline,
        expected_free,
        pages.table_frames(),
        STACK_PAGES,
        memory.frames.metadata_bytes(),
        core::mem::size_of::<[u64; BITMAP_WORDS]>(),
        pages.mappings()
    ));
    serial::print(format_args!(
        "M2CHECK exhausted_reclaimed={} reserved_excluded=1 map_unmap=1 tlb=1 readonly=1 nx=1 guards=2 wx_aliases=1 faults={}\n",
        acquired,
        faults::count()
    ));
    if cfg!(feature = "boot-unexpected-fault") {
        // SAFETY: the scratch mapping has been removed and invalidated above.
        unsafe {
            faults::trigger_unexpected(TEST_VIRTUAL);
        }
    }
    serial::print(format_args!("M2 PASS\n"));
    crate::m3::run(memory, pages);
    crate::m4::run(memory, pages);
    machine::exit(true)
}

fn assert_usable_frame(address: u64, regions: &[Region]) {
    assert_ne!(address, 0);
    let end = address + PAGE_SIZE;
    assert!(
        regions.iter().any(|region| region.kind == 0
            && region.base <= address
            && end <= region.base + region.length),
        "frame outside usable map"
    );
    assert!(
        !regions.iter().any(|region| region.kind != 0
            && region.base < end
            && address < region.base + region.length),
        "frame overlaps a reserved region"
    );
}
