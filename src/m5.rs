//! M5 A/B experiment using real pages, real CR3s and actual ring-3 reads/COW.
//! One explicitly opted-in policy store is reused only after complete teardown.
use super::*;
use core::arch::asm;
use shared_first_os::policy::{BYTES, Memory, Policy};

// SAFETY: linker-delimited position-independent fixture runs only from a private
// user RX page; no kernel symbol or privileged instruction is used by the fixture.
global_asm!(include_str!("m5_user.S"));
unsafe extern "C" {
    static m5_user_start: u8;
    static m5_user_end: u8;
}
type PolicyObjects = ObjectStore<Policy<Hardware>, 4>;
static mut POLICY_OBJECTS: PolicyObjects = ObjectStore::new(Policy::new(Hardware));

impl Memory for Hardware {
    fn read_page(&self, frame: u64, output: &mut [u8; BYTES]) {
        runtime(|state| {
            let (memory, _, _) = state.parts();
            assert!(memory.frames.is_allocated(frame));
            for (index, byte) in output.iter_mut().enumerate() {
                // SAFETY: retained initialized physical page in HHDM. Single
                // CPU, no live user execution or Rust reference to these bytes.
                *byte = unsafe {
                    ((memory.hhdm + frame) as *const u8)
                        .add(index)
                        .read_volatile()
                };
            }
        });
    }
    fn free_frames(&self) -> usize {
        runtime(|state| state.parts().0.frames.free())
    }
}

pub(super) fn ticks() -> u64 {
    let low: u32;
    let high: u32;
    // SAFETY: x86-64 QEMU target supports fences and RDTSC. No memory is touched;
    // omit nomem to preserve the compiler barrier around the measured work.
    unsafe {
        asm!("mfence", "lfence", "rdtsc", "lfence", out("eax") low, out("edx") high, options(nostack));
    }
    (u64::from(high) << 32) | u64::from(low)
}
fn datum(workload: usize, object: usize, offset: usize) -> u8 {
    match workload {
        0 => 0x5a,
        1 => (object * 4 + offset / BYTES + 1) as u8,
        _ => {
            let mut value = ((object as u64) << 32) | offset as u64;
            value = value.wrapping_add(0x9e3779b97f4a7c15);
            value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
            (value ^ (value >> 31)) as u8
        }
    }
}
fn experiment(store: &mut PolicyObjects, workload: usize, sample: usize, enabled: bool) {
    let baseline = accounting();
    store.backend_mut().configure(enabled);
    let mut handles = [Handle {
        slot: 0,
        generation: 0,
    }; 4];
    let mut sums = [0u64; 4];
    let mut input_hash = 0xcbf29ce484222325u64;
    // Checksumming is outside the create timer; generation inside create is
    // included for both modes. Only a one-page initialization buffer is used.
    for (object, sum) in sums.iter_mut().enumerate() {
        for offset in 0..4 * BYTES {
            let byte = datum(workload, object, offset);
            *sum += byte as u64;
            input_hash = (input_hash ^ byte as u64).wrapping_mul(0x100000001b3);
        }
    }
    let start = ticks();
    for (object, handle) in handles.iter_mut().enumerate() {
        *handle = store
            .create_initialized(object % 2, 4 * BYTES, |offset, page| {
                for (index, byte) in page.iter_mut().enumerate() {
                    *byte = datum(workload, object, offset + index);
                }
            })
            .unwrap();
    }
    let create_ticks = ticks() - start;
    let created = store.backend().stats();
    assert_eq!(
        created.resident,
        if enabled && workload == 0 { 1 } else { 16 }
    );
    assert_eq!(accounting().0, baseline.0 + created.resident);
    store.backend_mut().maintain(0);
    assert_eq!(store.backend().stats().attempts, 0);
    let free_before = Hardware.free_frames();
    let watermark = free_before + 32; // controlled pressure threshold, not fake OOM
    let start = ticks();
    store.backend_mut().maintain(watermark);
    store.backend_mut().maintain(watermark); // rejected immutable pages are not retried
    let pressure_ticks = ticks() - start;
    let cold = store.backend().stats();
    let compressible = enabled && workload != 2;
    assert_eq!(
        cold.resident,
        if compressible { 0 } else { created.resident }
    );
    assert_eq!(
        cold.compressed_pages,
        if compressible { created.resident } else { 0 }
    );
    assert_eq!(cold.compressed_bytes, cold.compressed_pages * 3);
    assert_eq!(accounting().0, baseline.0 + cold.resident);

    let start_code = ptr::addr_of!(m5_user_start) as usize;
    let code_length = ptr::addr_of!(m5_user_end) as usize - start_code;
    let mut writer = spawn_image(0, handles[0], 0, 0, start_code, code_length, "M5").unwrap();
    let mut reader = spawn_image(1, handles[1], 0, 0, start_code, code_length, "M5").unwrap();
    let mut counts = Counters {
        prefix: "M5",
        ..Counters::default()
    };
    let start = ticks();
    assert_eq!(
        drive(store, 0, &mut writer, &mut counts),
        Event::Yield(0, sums[0])
    );
    assert_eq!(
        drive(store, 1, &mut reader, &mut counts),
        Event::Yield(0, sums[1])
    );
    let map_read_ticks = ticks() - start;
    let mapped = store.backend().stats();
    assert_eq!(mapped.mappings, 8);
    assert_eq!(
        mapped.restored,
        if compressible {
            if workload == 0 { 1 } else { 8 }
        } else {
            0
        }
    );
    let allocated_mapped = accounting();
    assert_eq!(
        allocated_mapped.0,
        baseline.0 + mapped.resident + 5 + allocated_mapped.1 - baseline.1
    );
    assert_eq!(allocated_mapped.2, baseline.2 + 13);
    store.backend_mut().maintain(watermark);
    assert_eq!(store.backend().stats().resident, mapped.resident);
    assert_eq!(accounting(), allocated_mapped); // all restored pages are pinned
    for page in 0..4 {
        let first = translation(0, OBJECT + page * PAGE_SIZE).unwrap();
        let second = translation(1, OBJECT + page * PAGE_SIZE).unwrap();
        assert_eq!(first.1, Permissions::UserReadOnly);
        assert_eq!(second.1, Permissions::UserReadOnly);
        assert_eq!(first.0 == second.0, enabled && workload == 0);
    }
    let original = translation(0, OBJECT).unwrap().0;
    let first_byte = datum(workload, 0, 0) as u64;
    let start = ticks();
    assert_eq!(
        drive(store, 0, &mut writer, &mut counts),
        Event::Yield(1, sums[0] - first_byte + (first_byte ^ 255))
    );
    assert_eq!(
        drive(store, 1, &mut reader, &mut counts),
        Event::Yield(1, sums[1])
    );
    let cow_read_ticks = ticks() - start;
    assert_eq!(counts.cow, 1);
    assert_ne!(translation(0, OBJECT).unwrap().0, original);
    assert_eq!(store.backend().stats().resident, mapped.resident + 1);
    assert_eq!(accounting().0, allocated_mapped.0 + 1);
    let cow = store.backend().stats();
    let start = ticks();
    store.revoke(0, handles[0]).unwrap();
    assert!(translation(0, OBJECT).is_err());
    // Independently created object retains authority even when its page content
    // was interned with the revoked object. Reader still executes in CPL3.
    assert_eq!(
        drive(store, 1, &mut reader, &mut counts),
        Event::Yield(2, sums[1])
    );
    assert_eq!(
        drive(store, 0, &mut writer, &mut counts),
        Event::Fault(14, 4, OBJECT)
    );
    assert_eq!(drive(store, 1, &mut reader, &mut counts), Event::Exit(0));
    finish(store, 0, writer);
    finish(store, 1, reader);
    let cleanup_ticks = ticks() - start;
    assert_eq!(accounting(), baseline);
    let final_stats = store.backend().stats();
    assert_eq!(
        final_stats.resident
            + final_stats.compressed_pages
            + final_stats.references
            + final_stats.mappings,
        0
    );
    assert_eq!(store.stats().payload_frames + store.stats().capabilities, 0);
    serial::print(format_args!(
        "M5RUN workload={} sample={} enabled={} input_hash={:#x} logical_pages=16 created={} cold={} compressed_pages={} compressed_bytes={} mapped={} mappings=8 cow={} hits={} probes={} bypasses={} attempts={} rejected={} restored={} metadata={} arena={} object_metadata={} free_before={} watermark={} before={} after={} mapped_allocated={} extra_tables={} task_frames=5 final_tables={} final_mappings={} create_ticks={} pressure_ticks={} map_read_ticks={} cow_read_ticks={} cleanup_ticks={} isolation=1 reclaimed=1\n",
        workload,
        sample,
        usize::from(enabled),
        input_hash,
        created.resident,
        cold.resident,
        cold.compressed_pages,
        cold.compressed_bytes,
        mapped.resident,
        cow.resident,
        final_stats.hits,
        final_stats.probes,
        final_stats.bypasses,
        final_stats.attempts,
        final_stats.rejected,
        final_stats.restored,
        final_stats.metadata_bytes,
        final_stats.arena_bytes,
        store.stats().metadata_bytes,
        free_before,
        watermark,
        baseline.0,
        accounting().0,
        allocated_mapped.0,
        allocated_mapped.1 - baseline.1,
        accounting().1,
        accounting().2,
        create_ticks,
        pressure_ticks,
        map_read_ticks,
        cow_read_ticks,
        cleanup_ticks
    ));
}

pub(super) fn run() {
    let baseline = accounting();
    // SAFETY: one CPU, IF=0, one invocation after M4 teardown; no other access to
    // the statically initialized store. No reference to it reaches user space.
    let store = unsafe { &mut *ptr::addr_of_mut!(POLICY_OBJECTS) };
    for workload in 0..3 {
        for sample in 0..3 {
            // Alternate A/B order to expose, not eliminate, ordering effects.
            for enabled in if sample % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                experiment(store, workload, sample, enabled);
            }
        }
    }
    assert_eq!(accounting(), baseline);
    serial::print(format_args!(
        "M5CHECK workloads=3 samples=3 runs=18 ring3=1 independent_revoke=1 pinned=1 cow=1 cleanup=1\nM5 PASS\n"
    ));
}
