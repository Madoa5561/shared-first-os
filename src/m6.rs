//! Controlled baselines, not a new raw-memory userspace API. All frames and raw
//! COW authority belong to this kernel fixture. Actual user reads/writes use the
//! same CPU transitions and paging protections in every comparison mode.
use super::*;
use core::mem::size_of;
use m5::ticks;

const MAX: usize = 64;
const SAMPLES: usize = 8;
type BenchStore = ObjectStore<Hardware, MAX>;
static mut BENCH_STORE: BenchStore = ObjectStore::new(Hardware);
struct Raw {
    source: [u64; MAX],
    private: [u64; MAX],
}
static mut RAW: Raw = Raw {
    source: [0; MAX],
    private: [0; MAX],
};

// SAFETY: bounded position-independent fixture, copied to the same isolated RX
// user page as M4/M5; its register inputs are prepared by the kernel harness.
global_asm!(include_str!("m6_user.S"));
unsafe extern "C" {
    static m6_user_start: u8;
    static m6_user_end: u8;
    static __kernel_end: u8;
}
#[derive(Default)]
struct Execution {
    faults: usize,
    calls: usize,
    handler_ticks: u64,
    handler_max: u64,
    roots: [u64; 2],
    cs: [u64; 2],
}
struct Trial {
    mode: usize, // 0 eager-private, 1 raw shared RO, 2 raw COW, 3 object RO, 4 object COW
    pages: usize,
    dirty: usize,
}

// No serial output is reachable from this runner's successful timed paths.
fn resume(
    store: &mut BenchStore,
    raw: &mut Raw,
    trial: &Trial,
    task: usize,
    value: &mut Task,
    execution: &mut Execution,
) -> Event {
    for _ in 0..128 {
        assert!(value.context.valid_return());
        let (root, kernel) = roots(task);
        // SAFETY: same exclusive single-CPU/IF=0 Context and retained supervisor
        // kernel mappings as M4; no memory borrow crosses execution of user code.
        let (address, observed) = unsafe { user_cpu::enter(&mut value.context, root, kernel) };
        assert_eq!(observed, root);
        assert_eq!(user_cpu::current_root(), kernel);
        assert_eq!(value.context.cs & 3, 3);
        execution.roots[task] = observed;
        execution.cs[task] = value.context.cs & 3;
        if value.context.vector == 14 {
            assert_eq!((task, value.context.error), (0, 7));
            assert!(address >= OBJECT && address < OBJECT + trial.dirty as u64 * PAGE_SIZE);
            let start = ticks();
            match trial.mode {
                2 => {
                    let index = ((address - OBJECT) / PAGE_SIZE) as usize;
                    assert_eq!(raw.private[index], 0);
                    let copy = Hardware.copy_private(raw.source[index]).unwrap();
                    Hardware
                        .replace_private(task, address & !(PAGE_SIZE - 1), raw.source[index], copy)
                        .unwrap();
                    raw.private[index] = copy;
                }
                4 => store.resolve_cow(task, address).unwrap(),
                _ => panic!("M6 unexpected write fault"),
            }
            let elapsed = ticks() - start;
            execution.handler_ticks += elapsed;
            execution.handler_max = execution.handler_max.max(elapsed);
            execution.faults += 1;
            continue;
        }
        assert_eq!(value.context.vector, 128, "M6 unexpected user exception");
        execution.calls += 1;
        match value.context.rax {
            2 => {
                value.context.rax = 0;
                return Event::Yield(value.context.rdi, value.context.rsi);
            }
            8 => return Event::Exit(value.context.rdi),
            _ => panic!("M6 fixture syscall outside benchmark ABI"),
        }
    }
    panic!("M6 fixture failed to yield");
}

fn sample(store: &mut BenchStore, raw: &mut Raw, trial: &Trial, round: usize, order: usize) {
    let Trial { mode, pages, dirty } = *trial;
    assert!(
        raw.source
            .iter()
            .chain(raw.private.iter())
            .all(|&frame| frame == 0)
    );
    assert_eq!(store.stats().capabilities, 0);
    let baseline = accounting();
    runtime(|state| state.parts().0.frames.reset_peak());
    let mut handles = [Handle {
        slot: 0,
        generation: 0,
    }; 2];
    let start = ticks();
    if mode >= 3 {
        handles[0] = store
            .create_initialized(0, pages * PAGE_SIZE as usize, |offset, bytes| {
                bytes.fill((offset / PAGE_SIZE as usize + 1) as u8);
            })
            .unwrap();
        handles[1] = store.grant(0, handles[0], 1, Rights::READ_MAP).unwrap();
    } else {
        for index in 0..pages {
            raw.source[index] = Hardware.allocate(&[(index + 1) as u8; 4096]).unwrap();
            Hardware.seal(raw.source[index]).unwrap();
            if mode == 0 {
                raw.private[index] = Hardware.copy_private(raw.source[index]).unwrap();
            }
        }
    }
    let create_ticks = ticks() - start;
    let start_code = ptr::addr_of!(m6_user_start) as usize;
    let length = ptr::addr_of!(m6_user_end) as usize - start_code;
    let mut writer = spawn_image(0, handles[0], 0, 0, start_code, length, "M6").unwrap();
    let mut reader = spawn_image(1, handles[1], 0, 0, start_code, length, "M6").unwrap();
    for task in [&mut writer, &mut reader] {
        task.context.r13 = pages as u64 * PAGE_SIZE;
        task.context.r14 = dirty as u64;
    }
    let start = ticks();
    if mode >= 3 {
        store
            .map(
                0,
                handles[0],
                OBJECT,
                if mode == 4 { Rights::COW } else { Rights::READ },
            )
            .unwrap();
        store.map(1, handles[1], OBJECT, Rights::READ).unwrap();
    } else {
        runtime(|state| {
            let (memory, _, spaces) = state.parts();
            for index in 0..pages {
                for (task, space) in spaces[..2].iter_mut().enumerate() {
                    let writable = mode == 0 && task == 0;
                    space
                        .as_mut()
                        .unwrap()
                        .map(
                            memory,
                            OBJECT + index as u64 * PAGE_SIZE,
                            if writable {
                                raw.private[index]
                            } else {
                                raw.source[index]
                            },
                            if writable {
                                Permissions::UserReadWrite
                            } else {
                                Permissions::UserReadOnly
                            },
                        )
                        .unwrap();
                }
            }
        });
    }
    let map_ticks = ticks() - start;
    let mapped = accounting();
    assert_eq!(mapped.1 - baseline.1, 14);
    assert_eq!(mapped.2 - baseline.2, pages * 2 + 5);
    for index in 0..pages {
        let address = OBJECT + index as u64 * PAGE_SIZE;
        let first = translation(0, address).unwrap();
        let second = translation(1, address).unwrap();
        assert_eq!(first.0 == second.0, mode != 0);
        assert_eq!(
            first.1,
            if mode == 0 {
                Permissions::UserReadWrite
            } else {
                Permissions::UserReadOnly
            }
        );
        assert_eq!(second.1, Permissions::UserReadOnly);
    }
    let mut execution = Execution::default();
    let sum = PAGE_SIZE * (pages * (pages + 1) / 2) as u64;
    let start = ticks();
    let first = resume(store, raw, trial, 0, &mut writer, &mut execution);
    let second = resume(store, raw, trial, 1, &mut reader, &mut execution);
    let read_ticks = ticks() - start;
    assert_eq!(first, Event::Yield(0, sum));
    assert_eq!(second, Event::Yield(0, sum));
    let start = ticks();
    let first = resume(store, raw, trial, 0, &mut writer, &mut execution);
    let second = resume(store, raw, trial, 1, &mut reader, &mut execution);
    let write_read_ticks = ticks() - start;
    let changed = sum + (dirty * 255 - dirty * (dirty + 1)) as u64;
    assert_eq!(first, Event::Yield(1, changed));
    assert_eq!(second, Event::Yield(1, sum));
    let expected_faults = if mode == 2 || mode == 4 { dirty } else { 0 };
    assert_eq!(execution.faults, expected_faults);
    for index in 0..pages {
        let address = OBJECT + index as u64 * PAGE_SIZE;
        let first = translation(0, address).unwrap();
        let second = translation(1, address).unwrap();
        let private = mode == 0 || index < dirty;
        assert_eq!(first.0 != second.0, private);
        assert_eq!(
            first.1,
            if private {
                Permissions::UserReadWrite
            } else {
                Permissions::UserReadOnly
            }
        );
        assert_eq!(second.1, Permissions::UserReadOnly);
    }
    let payload = if mode == 0 {
        pages * 2
    } else {
        pages + expected_faults
    };
    let peak = runtime(|state| state.parts().0.frames.peak());
    assert_eq!(peak, baseline.0 + 19 + payload);
    assert_eq!(accounting().0, peak);
    assert_eq!(
        resume(store, raw, trial, 0, &mut writer, &mut execution),
        Event::Exit(0)
    );
    assert_eq!(
        resume(store, raw, trial, 1, &mut reader, &mut execution),
        Event::Exit(0)
    );
    assert_eq!(execution.calls, 6);
    let start = ticks();
    if mode >= 3 {
        store.unmap(0, handles[0], OBJECT).unwrap();
        store.unmap(1, handles[1], OBJECT).unwrap();
    } else {
        for index in 0..pages {
            for task in 0..2 {
                let expected = if task == 0 && raw.private[index] != 0 {
                    raw.private[index]
                } else {
                    raw.source[index]
                };
                assert_eq!(
                    Hardware.unmap(task, OBJECT + index as u64 * PAGE_SIZE),
                    expected
                );
            }
            if raw.private[index] != 0 {
                Hardware.destroy(raw.private[index]);
                raw.private[index] = 0;
            }
        }
    }
    let unmap_ticks = ticks() - start;
    let start = ticks();
    if mode >= 3 {
        store.release(0, handles[0]).unwrap();
        store.release(1, handles[1]).unwrap();
    } else {
        for frame in &mut raw.source[..pages] {
            Hardware.destroy(*frame);
            *frame = 0;
        }
    }
    let release_ticks = ticks() - start;
    destroy_space(0, &writer.frames);
    destroy_space(1, &reader.frames);
    assert_eq!(accounting(), baseline);
    assert_eq!(store.stats().capabilities + store.stats().payload_frames, 0);
    assert_eq!(runtime(|state| state.parts().0.frames.peak()), peak);
    serial::print(format_args!(
        "M6RUN pages={} dirty={} mode={} round={} order={} warmup={} root0={:#x} root1={:#x} cs0={} cs1={} input_sum={} writer_sum={} reader_sum={} payload_peak={} pool_peak={} before={} after={} extra_tables=14 task_frames=5 mapping_pages={} final_tables={} final_mappings={} metadata={} faults={} copies={} calls={} create_ticks={} map_ticks={} read_ticks={} write_read_ticks={} handler_ticks={} handler_max={} unmap_ticks={} release_ticks={} isolation=1 cleanup=1\n",
        pages,
        dirty,
        mode,
        round,
        order,
        usize::from(round == 0),
        execution.roots[0],
        execution.roots[1],
        execution.cs[0],
        execution.cs[1],
        sum,
        changed,
        sum,
        payload,
        peak,
        baseline.0,
        accounting().0,
        pages * 2,
        accounting().1,
        accounting().2,
        if mode >= 3 {
            store.stats().metadata_bytes
        } else {
            size_of::<Raw>()
        },
        execution.faults,
        if mode == 0 { pages } else { expected_faults },
        execution.calls,
        create_ticks,
        map_ticks,
        read_ticks,
        write_read_ticks,
        execution.handler_ticks,
        execution.handler_max,
        unmap_ticks,
        release_ticks
    ));
}

pub(super) fn run() {
    // SAFETY: single serialized invocation, IF=0, all M4/M5 tasks have exited;
    // neither of these kernel-private static stores is exposed to user code.
    let (store, raw) = unsafe {
        (
            &mut *ptr::addr_of_mut!(BENCH_STORE),
            &mut *ptr::addr_of_mut!(RAW),
        )
    };
    let kernel_bytes =
        ptr::addr_of!(__kernel_end) as usize - ptr::addr_of!(__kernel_start) as usize;
    let mut fence_min = u64::MAX;
    for _ in 0..32 {
        let start = ticks();
        fence_min = fence_min.min(ticks() - start);
    }
    serial::print(format_args!(
        "M6CONFIG samples={} warmups=1 sizes=2 conditions=22 runs=198 kernel_image_bytes={} raw_metadata={} object_metadata={} reserved_bench_bytes={} task_context_bytes={} fence_min={} file_pages=0 compressed_bytes=0 timed_serial=0\n",
        SAMPLES,
        kernel_bytes,
        size_of::<Raw>(),
        store.stats().metadata_bytes,
        size_of::<Raw>() + size_of::<BenchStore>(),
        2 * size_of::<Task>(),
        fence_min
    ));
    for pages in [4, MAX] {
        for dirty in [0, 1, pages] {
            let modes: &[usize] = if dirty == 0 {
                &[0, 1, 2, 3, 4]
            } else {
                &[0, 2, 4]
            };
            for round in 0..=SAMPLES {
                for order in 0..modes.len() {
                    let mode = modes[(order + round) % modes.len()];
                    sample(store, raw, &Trial { mode, pages, dirty }, round, order);
                }
            }
        }
    }
    serial::print(format_args!(
        "M6CHECK measured=176 warmups=22 peak_tracking=1 checksum=1 physical_identity=1 isolation=1 cleanup=1\nM6 PASS\n"
    ));
}
