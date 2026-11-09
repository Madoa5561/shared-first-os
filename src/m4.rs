//! Real ring-3 integration. All mutable state is serialized on the boot CPU;
//! user code runs only between completed kernel operations. Every trap reloads
//! kernel CR3 (no PCID/PGE) before any page-table edit or frame reclamation.
use crate::{
    m2::PhysicalMemory,
    serial,
    user_cpu::{self, Context},
};
use core::{arch::global_asm, ptr};
use shared_first_os::{
    frame::PAGE_SIZE,
    object::{Backend, BackendError, Error, Handle, ObjectStore, Rights, TASKS},
    paging::{PageTables, PagingError, Permissions, TableMemory},
};

const OBJECT: u64 = 0x4000_0000;
const LENGTH: usize = 4 * 1024 * 1024;
const COUNT: usize = LENGTH / PAGE_SIZE as usize;
const CODE: u64 = 0x40_0000;
const STACK: u64 = 0x80_0000;
const SECRET: u64 = 0x90_0000;
const SUM: u64 = LENGTH as u64 * 0x5a;
const ERR: u64 = u64::MAX;

#[path = "m5.rs"]
mod m5;
#[path = "m6.rs"]
mod m6;
#[path = "m7.rs"]
mod m7;

// SAFETY: bounded position-independent fixture, executed only in CPL3 from its
// own RX page. It references no kernel symbols and cannot change privileged state.
global_asm!(include_str!("user_program.S"));
unsafe extern "C" {
    static m4_user_start: u8;
    static m4_user_end: u8;
    static __kernel_start: u8;
}

struct Hardware;
type Objects = ObjectStore<Hardware, COUNT>;
// Const initialization avoids constructing the large bounded arrays on the stack.
static mut OBJECTS: Objects = Objects::new(Hardware);
struct Runtime {
    memory: *mut PhysicalMemory,
    kernel: *mut PageTables,
    spaces: [Option<PageTables>; TASKS],
    fail_copy: bool,
}
static mut RUNTIME: Runtime = Runtime {
    memory: ptr::null_mut(),
    kernel: ptr::null_mut(),
    spaces: [const { None }; TASKS],
    fail_copy: false,
};
impl Runtime {
    fn parts(
        &mut self,
    ) -> (
        &mut PhysicalMemory,
        &mut PageTables,
        &mut [Option<PageTables>; TASKS],
    ) {
        assert!(!self.memory.is_null() && !self.kernel.is_null());
        // SAFETY: run lends disjoint M2 state fields until final teardown. Their
        // original references remain suspended. This scoped borrow is the only
        // access, never reentrant; spaces is separate persistent Runtime storage.
        unsafe { (&mut *self.memory, &mut *self.kernel, &mut self.spaces) }
    }
}
fn runtime<T>(operation: impl FnOnce(&mut Runtime) -> T) -> T {
    // SAFETY: sole CPU, IF=0; trap assembly never accesses this Rust state.
    // Closures do not reenter runtime and cannot return a borrowed reference.
    unsafe { operation(&mut *ptr::addr_of_mut!(RUNTIME)) }
}
fn paging_error(error: PagingError) -> BackendError {
    match error {
        PagingError::OutOfFrames => BackendError::NoMemory,
        PagingError::AlreadyMapped => BackendError::Conflict,
        _ => panic!("M4 paging invariant: {error:?}"),
    }
}
impl Backend for Hardware {
    fn allocate(&mut self, data: &[u8]) -> Option<u64> {
        assert!(data.len() <= PAGE_SIZE as usize);
        runtime(|state| {
            let (memory, _, _) = state.parts();
            let frame = memory.allocate_table()?;
            for (index, &byte) in data.iter().enumerate() {
                // SAFETY: new exclusive, zeroed, HHDM-mapped page, not yet exposed.
                unsafe {
                    ((memory.hhdm + frame) as *mut u8)
                        .add(index)
                        .write_volatile(byte);
                }
            }
            Some(frame)
        })
    }
    fn seal(&mut self, frame: u64) -> Result<(), BackendError> {
        runtime(|state| {
            let (memory, kernel, _) = state.parts();
            kernel
                .protect(memory, memory.hhdm + frame, Permissions::ReadOnly)
                .expect("seal HHDM");
        });
        Ok(())
    }
    fn destroy(&mut self, frame: u64) {
        runtime(|state| {
            let (memory, kernel, _) = state.parts();
            assert_eq!(user_cpu::current_root(), kernel.root());
            kernel
                .protect(memory, memory.hhdm + frame, Permissions::ReadWrite)
                .unwrap();
            for index in 0..512 {
                // SAFETY: all user aliases have been removed; CR3 return flushed
                // their TLB entries. Sole CPU owns the now-private writable page.
                unsafe {
                    memory.pointer(frame, index).write_volatile(0);
                }
            }
            memory.frames.release(frame).unwrap();
        });
    }
    fn valid_range(&self, task: usize, address: u64, pages: usize) -> bool {
        task < TASKS
            && address >= OBJECT
            && address + pages as u64 * PAGE_SIZE <= OBJECT + LENGTH as u64
            && runtime(|state| state.spaces[task].is_some())
    }
    fn map_readonly(&mut self, task: usize, address: u64, frame: u64) -> Result<(), BackendError> {
        runtime(|state| {
            let (memory, _, spaces) = state.parts();
            spaces[task]
                .as_mut()
                .unwrap()
                .map(memory, address, frame, Permissions::UserReadOnly)
                .map_err(paging_error)
        })
    }
    fn unmap(&mut self, task: usize, address: u64) -> u64 {
        runtime(|state| {
            let (memory, _, spaces) = state.parts();
            spaces[task]
                .as_mut()
                .unwrap()
                .unmap(memory, address)
                .unwrap()
        })
    }
    fn copy_private(&mut self, original: u64) -> Option<u64> {
        runtime(|state| {
            if state.fail_copy {
                return None;
            }
            let (memory, _, _) = state.parts();
            let frame = memory.allocate_table()?;
            for index in 0..512 {
                // SAFETY: original is retained immutable backing; destination
                // is an exclusive new RW/NX frame. Raw volatile accesses only.
                unsafe {
                    memory
                        .pointer(frame, index)
                        .write_volatile(memory.pointer(original, index).read_volatile());
                }
            }
            Some(frame)
        })
    }
    fn replace_private(
        &mut self,
        task: usize,
        address: u64,
        old: u64,
        new: u64,
    ) -> Result<(), BackendError> {
        runtime(|state| {
            let (memory, _, spaces) = state.parts();
            spaces[task]
                .as_mut()
                .unwrap()
                .replace(memory, address, old, new, Permissions::UserReadWrite)
                .map_err(paging_error)
        })
    }
}

struct Task {
    context: Context,
    frames: [u64; 3],
}
#[derive(Debug, PartialEq, Eq)]
enum Event {
    Yield(u64, u64),
    Exit(u64),
    Fault(u64, u64, u64),
    BadReturn,
}
struct Counters {
    prefix: &'static str,
    entries: usize,
    calls: usize,
    denied: usize,
    cow: usize,
    killed: usize,
    exited: usize,
}
impl Default for Counters {
    fn default() -> Self {
        Self {
            prefix: "M4",
            entries: 0,
            calls: 0,
            denied: 0,
            cow: 0,
            killed: 0,
            exited: 0,
        }
    }
}

fn spawn(task: usize, handle: Handle, attack: u64, target: u64) -> Result<Task, BackendError> {
    let start = ptr::addr_of!(m4_user_start) as usize;
    let length = ptr::addr_of!(m4_user_end) as usize - start;
    spawn_image(task, handle, attack, target, start, length, "M4")
}
// start/length are linker-delimited trusted fixture bytes, never user arguments.
fn spawn_image(
    task: usize,
    handle: Handle,
    attack: u64,
    target: u64,
    start: usize,
    length: usize,
    prefix: &str,
) -> Result<Task, BackendError> {
    assert!(length > 0 && length <= PAGE_SIZE as usize);
    let mut frames = [0; 3];
    let result = runtime(|state| {
        let (memory, kernel, spaces) = state.parts();
        assert!(spaces[task].is_none());
        let mut pages = PageTables::new_user(memory, kernel).map_err(paging_error)?;
        let setup = (|| {
            for (index, address) in [CODE, STACK, SECRET].into_iter().enumerate() {
                if index == 2 && task != 0 {
                    continue;
                }
                let frame = memory.allocate_table().ok_or(BackendError::NoMemory)?;
                frames[index] = frame;
                if index == 0 {
                    for offset in 0..length {
                        // SAFETY: linker-delimited immutable fixture is in mapped
                        // rodata; destination is a new, private zeroed frame.
                        unsafe {
                            ((memory.hhdm + frame) as *mut u8)
                                .add(offset)
                                .write_volatile((start as *const u8).add(offset).read());
                        }
                    }
                } else if index == 2 {
                    // SAFETY: initialize this exclusively owned task-private page.
                    unsafe {
                        memory.pointer(frame, 0).write_volatile(0x42);
                    }
                }
                let rights = if index == 0 {
                    Permissions::UserReadExecute
                } else {
                    Permissions::UserReadWrite
                };
                pages
                    .map(memory, address, frame, rights)
                    .map_err(paging_error)?;
                if index == 0 {
                    kernel
                        .protect(memory, memory.hhdm + frame, Permissions::ReadOnly)
                        .unwrap();
                }
            }
            Ok(())
        })();
        // Keep the root reachable for both success and rollback.
        spaces[task] = Some(pages);
        setup
    });
    if let Err(error) = result {
        destroy_space(task, &frames);
        return Err(error);
    }
    let mut context = Context::new(CODE, STACK + PAGE_SIZE - 16);
    context.r12 = task as u64;
    context.r13 = handle.encode();
    context.r14 = attack;
    context.r15 = target;
    serial::print(format_args!(
        "{}TASK task={} root={:#x} code={:#x} stack={:#x} attack={}\n",
        prefix,
        task,
        roots(task).0,
        frames[0],
        frames[1],
        attack
    ));
    Ok(Task { context, frames })
}

fn destroy_space(task: usize, frames: &[u64; 3]) {
    for (index, address) in [CODE, STACK, SECRET].into_iter().enumerate() {
        if frames[index] == 0 {
            continue;
        }
        runtime(|state| {
            let (memory, _, spaces) = state.parts();
            if let Some(pages) = spaces[task].as_mut()
                && pages.lookup(memory, address).is_ok()
            {
                assert_eq!(pages.unmap(memory, address), Ok(frames[index]));
            }
        });
        Hardware.destroy(frames[index]);
    }
    runtime(|state| {
        let (memory, kernel, spaces) = state.parts();
        assert_eq!(user_cpu::current_root(), kernel.root());
        if let Some(pages) = spaces[task].take() {
            pages.destroy_user(memory);
        }
    });
}
fn finish<B: Backend, const N: usize>(objects: &mut ObjectStore<B, N>, task: usize, value: Task) {
    objects.release_task(task).unwrap();
    destroy_space(task, &value.frames);
}
fn roots(task: usize) -> (u64, u64) {
    runtime(|state| {
        let (_, kernel, spaces) = state.parts();
        (spaces[task].as_ref().unwrap().root(), kernel.root())
    })
}
fn accounting() -> (usize, usize, usize) {
    runtime(|state| {
        let (memory, kernel, spaces) = state.parts();
        (
            memory.frames.allocated(),
            kernel.table_frames()
                + spaces
                    .iter()
                    .flatten()
                    .map(PageTables::table_frames)
                    .sum::<usize>(),
            kernel.mappings()
                + spaces
                    .iter()
                    .flatten()
                    .map(PageTables::mappings)
                    .sum::<usize>(),
        )
    })
}
fn translation(task: usize, address: u64) -> Result<(u64, Permissions), PagingError> {
    runtime(|state| {
        let (memory, _, spaces) = state.parts();
        spaces[task].as_ref().unwrap().lookup(memory, address)
    })
}

fn drive<B: Backend, const N: usize>(
    objects: &mut ObjectStore<B, N>,
    task: usize,
    value: &mut Task,
    counts: &mut Counters,
) -> Event {
    for _ in 0..128 {
        if !value.context.valid_return() {
            return Event::BadReturn;
        }
        let (root, kernel) = roots(task);
        // SAFETY: task root retains supervisor kernel mappings; Task Context is
        // exclusively borrowed on a live kernel stack. No outstanding store or
        // runtime borrow crosses user execution. All interrupts remain disabled.
        let (address, observed) = unsafe { user_cpu::enter(&mut value.context, root, kernel) };
        assert_eq!(observed, root);
        assert_eq!(user_cpu::current_root(), kernel);
        counts.entries += 1;
        let context = &mut value.context;
        assert_eq!(context.cs & 3, 3);
        if context.vector == 14 && context.error == 7 {
            match objects.resolve_cow(task, address) {
                Ok(()) => {
                    counts.cow += 1;
                    serial::print(format_args!(
                        "{}COW task={} address={:#x} error=7 cs=3 copies={}\n",
                        counts.prefix, task, address, counts.cow
                    ));
                    continue; // Retry the actual faulting user instruction.
                }
                Err(Error::Backend(BackendError::NoMemory)) => {
                    serial::print(format_args!(
                        "{}COWFAIL task={} allocation_denied=1\n",
                        counts.prefix, task
                    ));
                }
                Err(_) => {}
            }
        }
        if context.vector != 128 {
            counts.killed += 1;
            serial::print(format_args!(
                "{}FAULT task={} vector={} error={} address={:#x} cs=3 killed=1\n",
                counts.prefix,
                task,
                context.vector,
                context.error,
                if context.vector == 14 { address } else { 0 }
            ));
            return Event::Fault(
                context.vector,
                context.error,
                if context.vector == 14 { address } else { 0 },
            );
        }
        counts.calls += 1;
        let number = context.rax;
        if number == 2 {
            context.rax = 0;
            serial::print(format_args!(
                "{}USER task={} phase={} checksum={} cs=3 root={:#x}\n",
                counts.prefix, task, context.rdi, context.rsi, observed
            ));
            return Event::Yield(context.rdi, context.rsi);
        }
        if number == 8 {
            counts.exited += 1;
            serial::print(format_args!(
                "{}EXIT task={} status={} cs=3\n",
                counts.prefix, task, context.rdi
            ));
            return Event::Exit(context.rdi);
        }
        let result = syscall(objects, task, number, context.rdi, context.rsi, context.rdx);
        context.rax = match result {
            Ok(value) => value,
            Err(error) => {
                counts.denied += 1;
                serial::print(format_args!(
                    "{}DENY task={} syscall={} reason={:?}\n",
                    counts.prefix, task, number, error
                ));
                ERR
            }
        };
    }
    Event::BadReturn // Bounded syscall storm; uncooperative loops need future preemption.
}

fn syscall<B: Backend, const N: usize>(
    objects: &mut ObjectStore<B, N>,
    caller: usize,
    number: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
) -> Result<u64, Error> {
    if number == 6 {
        return Ok(caller as u64);
    }
    if number == 9 {
        let length = usize::try_from(arg1).map_err(|_| Error::InvalidSize)?;
        let byte = u8::try_from(arg2).map_err(|_| Error::InvalidSize)?;
        let handle = objects.create_filled(caller, length, byte)?;
        serial::print(format_args!(
            "M4CREATE task={} bytes={} fill={}\n",
            caller, length, byte
        ));
        return Ok(handle.encode());
    }
    if !matches!(number, 1 | 3 | 4 | 5 | 7) {
        return Err(Error::Denied);
    }
    let handle = Handle::decode(arg1)?;
    match number {
        1 => objects.map(caller, handle, arg2, Rights::from_bits(arg3)?)?,
        3 => objects.release(caller, handle)?,
        4 => objects.revoke(caller, handle)?,
        5 => {
            let target = usize::try_from(arg2).map_err(|_| Error::InvalidTask)?;
            if target >= TASKS || !runtime(|state| state.spaces[target].is_some()) {
                return Err(Error::InvalidTask);
            }
            return objects
                .grant(caller, handle, target, Rights::from_bits(arg3)?)
                .map(Handle::encode);
        }
        7 => objects.unmap(caller, handle, arg2)?,
        _ => unreachable!(),
    }
    Ok(0)
}

pub fn run(memory: &mut PhysicalMemory, kernel: &mut PageTables) {
    let baseline = (
        memory.frames.allocated(),
        kernel.table_frames(),
        kernel.mappings(),
    );
    runtime(|state| {
        assert!(state.memory.is_null());
        state.memory = memory;
        state.kernel = kernel;
    });
    // SAFETY: single exclusive store borrow; statically initialized, never
    // exposed to ring 3. Runtime accesses disjoint state, not this object store.
    let objects = unsafe { &mut *ptr::addr_of_mut!(OBJECTS) };
    assert_eq!(objects.stats().objects, 0);
    user_cpu::init();
    let owner = objects.create_filled(0, LENGTH, 0x5a).unwrap();
    let reader = objects.grant(0, owner, 1, Rights::READ_MAP).unwrap();
    let mut writer_task = spawn(0, owner, 0, 0).unwrap();
    let mut reader_task = spawn(1, reader, 0, 0).unwrap();
    let mut counts = Counters::default();
    assert_ne!(roots(0).0, roots(1).0);
    assert_eq!(
        drive(objects, 0, &mut writer_task, &mut counts),
        Event::Yield(0, SUM)
    );
    assert_eq!(
        drive(objects, 1, &mut reader_task, &mut counts),
        Event::Yield(0, SUM)
    );
    for index in 0..COUNT {
        let address = OBJECT + index as u64 * PAGE_SIZE;
        let first = translation(0, address).unwrap();
        assert_eq!(first.1, Permissions::UserReadOnly);
        assert_eq!(translation(1, address), Ok(first));
        runtime(|state| {
            let (memory, kernel, _) = state.parts();
            assert_eq!(
                kernel.lookup(memory, memory.hhdm + first.0),
                Ok((first.0, Permissions::ReadOnly))
            );
        });
    }
    let shared = accounting();
    let stats = objects.stats();
    assert_eq!(stats.payload_frames, COUNT);
    assert_eq!(stats.mapping_pages, COUNT * 2);
    assert_eq!(stats.private_frames, 0);
    assert_eq!(shared.0 - baseline.0, COUNT + 5 + shared.1 - baseline.1);
    let original = translation(0, OBJECT).unwrap().0;
    serial::print(format_args!(
        "M4SHARED bytes={} pages={} task0_root={:#x} task1_root={:#x} first_frame={:#x} payload_frames={} mapping_pages={} metadata_bytes={} allocated={} table_frames={} task_frames=5\n",
        LENGTH,
        COUNT,
        roots(0).0,
        roots(1).0,
        original,
        stats.payload_frames,
        stats.mapping_pages,
        stats.metadata_bytes,
        shared.0,
        shared.1
    ));
    assert_eq!(
        drive(objects, 0, &mut writer_task, &mut counts),
        Event::Yield(1, SUM + 155)
    );
    assert_eq!(
        drive(objects, 1, &mut reader_task, &mut counts),
        Event::Yield(1, SUM)
    );
    let private = translation(0, OBJECT).unwrap();
    assert_ne!(private.0, original);
    assert_eq!(private.1, Permissions::UserReadWrite);
    assert_eq!(
        translation(1, OBJECT),
        Ok((original, Permissions::UserReadOnly))
    );
    for index in 1..COUNT {
        let address = OBJECT + index as u64 * PAGE_SIZE;
        assert_eq!(translation(0, address), translation(1, address));
    }
    runtime(|state| {
        let (memory, _, _) = state.parts();
        // SAFETY: both retained backing frames remain mapped in HHDM. Reading
        // raw bytes cannot alias Rust references; writer is suspended in kernel.
        unsafe {
            assert_eq!(
                ((memory.hhdm + original + 123) as *const u8).read_volatile(),
                0x5a
            );
            assert_eq!(
                ((memory.hhdm + private.0 + 123) as *const u8).read_volatile(),
                0xa7
            );
        }
    });
    assert_eq!(counts.cow, 1);
    assert_eq!(objects.stats().private_frames, 1);
    assert_eq!(objects.stats().payload_frames, COUNT + 1);
    assert_eq!(accounting(), (shared.0 + 1, shared.1, shared.2));
    serial::print(format_args!(
        "M4PRIVATE original={:#x} private={:#x} copied_pages=1 unchanged_pages={} payload_frames={} allocated={}\n",
        original,
        private.0,
        COUNT - 1,
        COUNT + 1,
        accounting().0
    ));

    let hhdm = runtime(|state| state.parts().0.hhdm);
    let attacks = [
        (0, 0, Event::Exit(0)),
        (1, 0, Event::Fault(14, 7, OBJECT)),
        (2, 0, Event::Fault(14, 21, OBJECT)),
        (
            3,
            ptr::addr_of!(__kernel_start) as u64,
            Event::Fault(14, 5, ptr::addr_of!(__kernel_start) as u64),
        ),
        (4, hhdm + original, Event::Fault(14, 5, hhdm + original)),
        (5, 0, Event::Fault(14, 4, SECRET)),
        (6, 0, Event::Fault(13, 0, 0)),
        (7, 0, Event::Fault(13, 0x40a, 0)),
        (8, 0, Event::Fault(14, 7, CODE)),
        (9, 0, Event::Fault(14, 4, STACK - PAGE_SIZE)),
        (10, 0, Event::Fault(7, 0, 0)),
        (11, 0, Event::Fault(14, 7, OBJECT)),
    ];
    for (case, target, expected) in attacks {
        let before = accounting();
        let rights = if case == 11 {
            Rights::READ_MAP.union(Rights::COW)
        } else {
            Rights::READ_MAP
        };
        let handle = objects.grant(0, owner, 2, rights).unwrap();
        let mut attack = spawn(2, handle, case, target).unwrap();
        runtime(|state| state.fail_copy = case == 11);
        assert_eq!(
            drive(objects, 2, &mut attack, &mut counts),
            expected,
            "attack case {case}"
        );
        runtime(|state| state.fail_copy = false);
        finish(objects, 2, attack);
        assert_eq!(accounting(), before);
        assert_eq!(objects.stats().capabilities, 2);
        serial::print(format_args!("M4ATTACK case={} reclaimed=1\n", case));
    }
    assert_eq!(
        drive(objects, 0, &mut writer_task, &mut counts),
        Event::Yield(2, 0)
    );
    assert_eq!(objects.stats().payload_frames, 0);
    assert_eq!(objects.stats().capabilities, 0);
    for task in 0..2 {
        for index in 0..COUNT {
            assert_eq!(
                translation(task, OBJECT + index as u64 * PAGE_SIZE),
                Err(PagingError::NotMapped)
            );
        }
    }
    assert_eq!(
        drive(objects, 0, &mut writer_task, &mut counts),
        Event::Fault(14, 4, OBJECT)
    );
    finish(objects, 0, writer_task);
    assert_eq!(
        drive(objects, 1, &mut reader_task, &mut counts),
        Event::Fault(14, 4, OBJECT)
    );
    finish(objects, 1, reader_task);
    assert_eq!(accounting(), baseline);
    assert_eq!(counts.killed, 13);
    assert_eq!(counts.exited, 1);
    serial::print(format_args!(
        "M4FRAMES before={} after={} tables_before={} tables_after={} mappings_before={} mappings_after={} payload_frames=0 private_frames=0 task_frames=0 capabilities=0\n",
        baseline.0,
        accounting().0,
        baseline.1,
        accounting().1,
        baseline.2,
        accounting().2
    ));
    serial::print(format_args!(
        "M4CHECK entries={} syscalls={} denied={} cow_faults={} killed={} exited={} shared_pages={} unchanged_pages={} revoke=1 cleanup=1 ring3=1\n",
        counts.entries,
        counts.calls,
        counts.denied,
        counts.cow,
        counts.killed,
        counts.exited,
        COUNT,
        COUNT - 1
    ));
    serial::print(format_args!("M4 PASS\n"));
    m5::run();
    m6::run();
    m7::run();
    runtime(|state| {
        assert!(state.spaces.iter().all(Option::is_none));
        state.memory = ptr::null_mut();
        state.kernel = ptr::null_mut();
    });
}
