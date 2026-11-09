//! Restricted bytecode shares kernel CR3, never native execution privilege.
//! See docs/M7-THREAT-MODEL.md. Native controls retain the M4 hardware boundary.
use super::*;
#[path = "m10.rs"]
mod m10;
#[path = "m11.rs"]
mod m11;
#[path = "m8.rs"]
mod m8;
#[path = "m9.rs"]
mod m9;
use core::mem::size_of;
use shared_first_os::{
    object::ReadBackend,
    sandbox::{Context as Vm, Fault, Host, Outcome, Program, instruction as ins},
};
type Bench = ObjectStore<Hardware, 64>;
static mut STORE: Bench = ObjectStore::new(Hardware);
#[derive(Clone, Copy)]
struct Trace {
    frames: [u64; 64],
    count: usize,
}
static mut TRACE: Trace = Trace {
    frames: [0; 64],
    count: 0,
};
unsafe extern "C" {
    static m6_user_start: u8;
    static m6_user_end: u8;
    static __kernel_end: u8;
}
fn trace() -> Trace {
    // SAFETY: single CPU, IF=0, no access from user/native trap code.
    unsafe { ptr::addr_of!(TRACE).read() }
}
fn reset_trace() {
    // SAFETY: serialized harness; no reference to this diagnostic state escapes.
    unsafe {
        (*ptr::addr_of_mut!(TRACE)).count = 0;
    }
}
fn current_cpl() -> u16 {
    let selector: u16;
    // SAFETY: reads the current code selector without changing CPU state.
    unsafe {
        core::arch::asm!("mov {0:x}, cs", out(reg) selector, options(nomem, nostack, preserves_flags));
    }
    selector & 3
}
impl ReadBackend for Hardware {
    fn sum_page(&self, frame: u64, offset: usize, length: usize) -> u64 {
        assert!(offset <= 4096 && length <= 4096 - offset);
        runtime(|state| {
            let (memory, kernel, _) = state.parts();
            assert_eq!(user_cpu::current_root(), kernel.root());
            assert_eq!(
                kernel.lookup(memory, memory.hhdm + frame).unwrap(),
                (frame, Permissions::ReadOnly)
            );
            // SAFETY: ObjectStore retains this sealed immutable page throughout
            // the synchronous read. Bytecode supplies no physical address. IF=0,
            // sole CPU, no release/revoke interleaves, no Rust reference is made.
            let mut sum = 0;
            for index in offset..offset + length {
                // SAFETY: retained sealed backing and checked page range above.
                sum +=
                    unsafe { ((memory.hhdm + frame + index as u64) as *const u8).read_volatile() }
                        as u64;
            }
            // SAFETY: diagnostic state exclusive to this serialized runner.
            unsafe {
                let log = &mut *ptr::addr_of_mut!(TRACE);
                assert!(log.count < log.frames.len());
                log.frames[log.count] = frame;
                log.count += 1;
            }
            sum
        })
    }
}
struct BoundHost<'a> {
    store: &'a Bench,
    task: usize,
}
impl Host for BoundHost<'_> {
    fn sum(&mut self, handle: u64, offset: u64, length: u64) -> Result<u64, Fault> {
        reset_trace();
        let handle = Handle::decode(handle).map_err(|_| Fault::Authority)?;
        self.store
            .read_sum(self.task, handle, offset, length)
            .map_err(|error| match error {
                Error::InvalidAddress | Error::Overflow => Fault::Bounds,
                _ => Fault::Authority,
            })
    }
}
fn code(handle: Handle, bytes: usize) -> [u8; 96] {
    flatten([
        ins(1, 1, 0, 0, handle.encode()),
        ins(2, 0, 1, 2, bytes as u64),
        ins(7, 0, 0, 0, 0),
        ins(2, 0, 1, 2, bytes as u64),
        ins(7, 0, 0, 0, 0),
        ins(0, 0, 0, 0, 0),
    ])
}
fn flatten<const N: usize, const B: usize>(words: [[u8; 16]; N]) -> [u8; B] {
    assert_eq!(B, N * 16);
    let mut bytes = [0; B];
    for (destination, source) in bytes.chunks_exact_mut(16).zip(words) {
        destination.copy_from_slice(&source);
    }
    bytes
}
fn native(task: usize, value: &mut Task, observed_root: &mut u64) -> Event {
    assert!(value.context.valid_return());
    let (root, kernel) = roots(task);
    // SAFETY: same exclusive Context, supervisor mappings and single-CPU
    // transitions as M4/M6. Native code remains CPL3 with its own root.
    let (_, observed) = unsafe { user_cpu::enter(&mut value.context, root, kernel) };
    assert_eq!(observed, root);
    assert_eq!(user_cpu::current_root(), kernel);
    assert_eq!(value.context.cs & 3, 3);
    assert_eq!(value.context.vector, 128);
    *observed_root = observed;
    match value.context.rax {
        2 => {
            value.context.rax = 0;
            Event::Yield(value.context.rdi, value.context.rsi)
        }
        8 => Event::Exit(value.context.rdi),
        _ => panic!("M7 unexpected native syscall"),
    }
}
fn sample(store: &mut Bench, pages: usize, mode: usize, round: usize) {
    let baseline = accounting();
    runtime(|state| state.parts().0.frames.reset_peak());
    let bytes = pages * 4096;
    let owner = store
        .create_initialized(0, bytes, |offset, page| {
            page.fill((offset / 4096 + 1) as u8)
        })
        .unwrap();
    let reader = store.grant(0, owner, 1, Rights::READ_MAP).unwrap();
    let sum = 4096 * (pages * (pages + 1) / 2) as u64;
    let mut observed = [0; 2];
    let (read_ticks, repeat_ticks, consumed, extra_tables, task_frames, extra_maps, cpl);
    if mode == 0 {
        let start = ptr::addr_of!(m6_user_start) as usize;
        let length = ptr::addr_of!(m6_user_end) as usize - start;
        let mut first = spawn_image(0, owner, 0, 0, start, length, "M7").unwrap();
        let mut second = spawn_image(1, reader, 0, 0, start, length, "M7").unwrap();
        for task in [&mut first, &mut second] {
            task.context.r13 = bytes as u64;
            task.context.r14 = 0;
        }
        store.map(0, owner, OBJECT, Rights::READ).unwrap();
        store.map(1, reader, OBJECT, Rights::READ).unwrap();
        for index in 0..pages {
            let address = OBJECT + index as u64 * PAGE_SIZE;
            let left = translation(0, address).unwrap();
            assert_eq!(left, translation(1, address).unwrap());
            assert_eq!(left.1, Permissions::UserReadOnly);
        }
        let start = m5::ticks();
        let left = native(0, &mut first, &mut observed[0]);
        let right = native(1, &mut second, &mut observed[1]);
        read_ticks = m5::ticks() - start;
        assert_eq!((left, right), (Event::Yield(0, sum), Event::Yield(0, sum)));
        let start = m5::ticks();
        let left = native(0, &mut first, &mut observed[0]);
        let right = native(1, &mut second, &mut observed[1]);
        repeat_ticks = m5::ticks() - start;
        assert_eq!((left, right), (Event::Yield(1, sum), Event::Yield(1, sum)));
        assert_eq!(native(0, &mut first, &mut observed[0]), Event::Exit(0));
        assert_eq!(native(1, &mut second, &mut observed[1]), Event::Exit(0));
        cpl = first.context.cs & 3;
        extra_tables = accounting().1 - baseline.1;
        extra_maps = accounting().2 - baseline.2;
        task_frames = accounting().0 - baseline.0 - extra_tables - pages;
        assert_eq!(
            (extra_tables, task_frames, extra_maps),
            (14, 5, pages * 2 + 5)
        );
        finish(store, 0, first);
        finish(store, 1, second);
        consumed = 0;
    } else {
        cpl = u64::from(current_cpl());
        assert_eq!(cpl, 0);
        let first_code = code(owner, bytes);
        let second_code = code(reader, bytes);
        let budget = 2 * bytes as u64 + 6;
        let mut first = Vm::new(Program::validate(&first_code).unwrap(), budget);
        let mut second = Vm::new(Program::validate(&second_code).unwrap(), budget);
        let mut first_host = BoundHost { store, task: 0 };
        let mut second_host = BoundHost { store, task: 1 };
        let mut elapsed = [0; 2];
        for duration in &mut elapsed {
            let start = m5::ticks();
            let left = first.run(&mut first_host);
            *duration = m5::ticks() - start;
            let first_trace = trace();
            observed[0] = user_cpu::current_root();
            let start = m5::ticks();
            let right = second.run(&mut second_host);
            *duration += m5::ticks() - start;
            let second_trace = trace();
            observed[1] = user_cpu::current_root();
            assert_eq!(
                (left, right),
                (Ok(Outcome::Yield(sum)), Ok(Outcome::Yield(sum)))
            );
            assert_eq!((first_trace.count, second_trace.count), (pages, pages));
            assert_eq!(&first_trace.frames[..pages], &second_trace.frames[..pages]);
            assert_eq!(observed, [runtime(|state| state.parts().1.root()); 2]);
        }
        assert_eq!(first.run(&mut first_host), Ok(Outcome::Halt(sum)));
        assert_eq!(second.run(&mut second_host), Ok(Outcome::Halt(sum)));
        consumed = 2 * budget - first.remaining() - second.remaining();
        assert_eq!(consumed, 4 * bytes as u64 + 12);
        [read_ticks, repeat_ticks] = elapsed;
        extra_tables = accounting().1 - baseline.1;
        extra_maps = accounting().2 - baseline.2;
        task_frames = accounting().0 - baseline.0 - pages;
        assert_eq!((extra_tables, task_frames, extra_maps), (0, 0, 0));
        store.release_task(0).unwrap();
        store.release_task(1).unwrap();
    }
    let peak = runtime(|state| state.parts().0.frames.peak());
    assert_eq!(peak, baseline.0 + pages + extra_tables + task_frames);
    assert_eq!(accounting(), baseline);
    assert_eq!(
        store.stats().capabilities + store.stats().payload_frames + store.stats().mapping_pages,
        0
    );
    serial::print(format_args!(
        "M7RUN pages={} mode={} round={} warmup={} root0={:#x} root1={:#x} cpl={} tables={} task_frames={} payload={} pool_peak={} before={} after={} maps={} final_tables={} final_maps={} sum0={} sum1={} repeat0={} repeat1={} fuel={} read_ticks={} repeat_ticks={} shared=1 cleanup=1\n",
        pages,
        mode,
        round,
        usize::from(round == 0),
        observed[0],
        observed[1],
        cpl,
        extra_tables,
        task_frames,
        pages,
        peak,
        baseline.0,
        accounting().0,
        extra_maps,
        baseline.1,
        baseline.2,
        sum,
        sum,
        sum,
        sum,
        consumed,
        read_ticks,
        repeat_ticks
    ));
}
fn rejected_program(name: &str, bytes: &[u8], expected: Fault) {
    assert_eq!(Program::validate(bytes).unwrap_err(), expected);
    serial::print(format_args!(
        "M7ATTACK name={} denied=1 unchanged=1\n",
        name
    ));
}
fn rejected_run(name: &str, store: &Bench, task: usize, bytes: &[u8], fuel: u64, expected: Fault) {
    reset_trace();
    let mut vm = Vm::new(Program::validate(bytes).unwrap(), fuel);
    let mut host = BoundHost { store, task };
    assert_eq!(vm.run(&mut host), Err(expected));
    assert_eq!(vm.run(&mut host), Err(Fault::Stopped));
    assert_eq!(trace().count, 0, "rejected access reached backend");
    serial::print(format_args!(
        "M7ATTACK name={} denied=1 unchanged=1\n",
        name
    ));
}
fn attacks(store: &mut Bench) {
    let baseline = accounting();
    let owner = store.create_filled(0, 4096, 7).unwrap();
    let reader = store.grant(0, owner, 1, Rights::READ).unwrap();
    let no_read = store.grant(0, owner, 2, Rights::MAP).unwrap();
    rejected_program("unknown_opcode", &ins(255, 0, 0, 0, 0), Fault::Opcode);
    rejected_program("native_opcode", &ins(15, 0, 0, 0, 0), Fault::Opcode);
    rejected_program("object_write", &ins(8, 0, 0, 0, 0), Fault::Opcode);
    rejected_program("bad_register", &ins(1, 4, 0, 0, 0), Fault::Register);
    let mut reserved = ins(0, 0, 0, 0, 0);
    reserved[15] = 1;
    rejected_program("reserved_bits", &reserved, Fault::Reserved);
    rejected_program("bad_jump", &ins(6, 0, 0, 0, 1), Fault::Jump);
    rejected_program("truncated", &[0; 15], Fault::Format);
    rejected_program("oversized", &[0; 272], Fault::Format);
    for (name, opcode, offset) in [
        ("local_read", 3, 64),
        ("local_write", 4, 64),
        ("local_overflow", 3, u64::MAX),
    ] {
        let bytes: [u8; 32] = flatten([ins(1, 1, 0, 0, offset), ins(opcode, 0, 1, 0, 0)]);
        rejected_run(name, store, 0, &bytes, 10, Fault::Bounds);
    }
    for (name, task, handle, offset, expected) in [
        ("object_bounds", 0, owner.encode(), 4095, Fault::Bounds),
        (
            "object_overflow",
            0,
            owner.encode(),
            u64::MAX,
            Fault::Bounds,
        ),
        ("foreign_handle", 3, owner.encode(), 0, Fault::Authority),
        (
            "forged_generation",
            0,
            owner.encode() + (1 << 16),
            0,
            Fault::Authority,
        ),
        ("missing_read", 2, no_read.encode(), 0, Fault::Authority),
        ("malformed_handle", 0, 1 << 63, 0, Fault::Authority),
    ] {
        let bytes: [u8; 48] = flatten([
            ins(1, 1, 0, 0, handle),
            ins(1, 2, 0, 0, offset),
            ins(2, 0, 1, 2, 2),
        ]);
        rejected_run(name, store, task, &bytes, 100, expected);
    }
    rejected_run("loop_fuel", store, 0, &ins(6, 0, 0, 0, 0), 100, Fault::Fuel);
    rejected_run("bulk_fuel", store, 0, &code(owner, 4096), 4097, Fault::Fuel);
    let local: [u8; 80] = flatten([
        ins(3, 0, 1, 0, 0),
        ins(7, 0, 0, 0, 0),
        ins(1, 0, 0, 0, 99),
        ins(4, 0, 1, 0, 0),
        ins(0, 0, 0, 0, 0),
    ]);
    let program = Program::validate(&local).unwrap();
    let mut writer = Vm::new(program, 10);
    let mut observer = Vm::new(program, 10);
    assert_eq!(
        writer.run(&mut BoundHost { store, task: 0 }),
        Ok(Outcome::Yield(0))
    );
    assert_eq!(
        writer.run(&mut BoundHost { store, task: 0 }),
        Ok(Outcome::Halt(99))
    );
    assert_eq!(
        observer.run(&mut BoundHost { store, task: 1 }),
        Ok(Outcome::Yield(0))
    );
    reset_trace();
    assert_eq!(store.read_sum(0, owner, 0, 4096), Ok(4096 * 7));
    serial::print(format_args!(
        "M7ATTACK name=scratch_isolation denied=1 unchanged=1\n"
    ));
    let reading = code(reader, 4096);
    let mut suspended = Vm::new(Program::validate(&reading).unwrap(), 10000);
    assert_eq!(
        suspended.run(&mut BoundHost { store, task: 1 }),
        Ok(Outcome::Yield(4096 * 7))
    );
    store.revoke(0, owner).unwrap();
    assert_eq!(
        suspended.run(&mut BoundHost { store, task: 1 }),
        Err(Fault::Authority)
    );
    assert_eq!(trace().count, 0);
    serial::print(format_args!(
        "M7ATTACK name=revoked_resume denied=1 unchanged=1\n"
    ));
    let replacement = store.create_filled(0, 4096, 9).unwrap();
    assert_eq!(owner.slot, replacement.slot);
    assert_ne!(owner.generation, replacement.generation);
    rejected_run(
        "stale_reuse",
        store,
        0,
        &code(owner, 4096),
        10000,
        Fault::Authority,
    );
    reset_trace();
    assert_eq!(store.read_sum(0, replacement, 0, 4096), Ok(4096 * 9));
    store.release_task(0).unwrap();
    assert_eq!(accounting(), baseline);
    assert_eq!(store.stats().capabilities + store.stats().payload_frames, 0);
}
pub(super) fn run() {
    // SAFETY: serialized invocation after M4/M5/M6 release all user tasks;
    // this independent static store is never exposed directly to bytecode.
    let store = unsafe { &mut *ptr::addr_of_mut!(STORE) };
    serial::print(format_args!(
        "M7CONFIG conditions=4 samples=8 warmups=1 runs=36 attacks=22 kernel_image_bytes={} object_metadata={} trace_bytes={} vm_context_bytes={} native_context_bytes={} bytecode_bytes=192 file_pages=0 compressed_bytes=0\n",
        ptr::addr_of!(__kernel_end) as usize - ptr::addr_of!(__kernel_start) as usize,
        store.stats().metadata_bytes,
        size_of::<Trace>(),
        2 * size_of::<Vm>(),
        2 * size_of::<Task>()
    ));
    attacks(store);
    for pages in [4, 64] {
        for round in 0..=8 {
            for order in 0..2 {
                sample(store, pages, (round + order) % 2, round);
            }
        }
    }
    serial::print(format_args!(
        "M7CHECK measured=32 warmups=4 attacks=22 shared=1 fuel=1 revocation=1 native_paging=1 cleanup=1\nM7 PASS\n"
    ));
    m8::run(store);
    m9::run(store);
    m10::run(store);
    m11::run(store);
}
