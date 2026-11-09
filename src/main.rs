#![no_std]
#![no_main]

mod boot;
mod faults;
mod m2;
mod m3;
mod m4;
mod machine;
mod serial;
mod user_cpu;

use core::panic::PanicInfo;
use shared_first_os::memory::{KIND_NAMES, Summary};

macro_rules! serialln {
    ($($arg:tt)*) => {{ serial::print(format_args!($($arg)*)); serial::print(format_args!("\n")); }};
}

// SAFETY: unique ELF entry symbol. Limine enters ring 0 with a valid SysV stack,
// mapped image and disabled interrupts. This entry never returns to the loader.
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    machine::disable_interrupts();
    serial::init();
    boot::check_revision().expect("Limine revision check");
    serialln!("Shared-First OS booted");
    let firmware = boot::firmware_kind().expect("firmware response");
    serialln!("BOOT base_revision=3 firmware={}", firmware);
    assert_eq!(firmware, 2, "M1 requires UEFI64");
    let map = boot::MemoryMap::get().expect("Limine memory map");
    let mut summary = Summary::default();
    for index in 0..map.len() {
        let region = map.region(index).expect("memory-map entry");
        summary.add(region).expect("memory-map accounting");
        serialln!(
            "MEMMAP index={} base={:#018x} length={:#018x} kind={}",
            index,
            region.base,
            region.length,
            region.kind
        );
    }
    for (index, name) in KIND_NAMES.iter().enumerate() {
        let totals = summary.kinds[index];
        serialln!(
            "MEMKIND name={} regions={} bytes={}",
            name,
            totals.regions,
            totals.bytes
        );
    }
    serialln!(
        "MEMSUMMARY entries={} usable_bytes={} reserved_bytes={} described_bytes={}",
        summary.all.regions,
        summary.kinds[0].bytes,
        summary.reserved.bytes,
        summary.all.bytes
    );
    serialln!(
        "ACCOUNTING reserved=all_non_usable; address_map_bytes_are_not_ram_usage; allocator=initialized_after_summary"
    );
    assert!(summary.kinds[0].bytes > 0, "no usable memory");
    assert!(summary.reserved.bytes > 0, "no reserved memory");
    if cfg!(feature = "boot-panic") {
        panic!("intentional boot-panic test");
    }
    serialln!("M1 PASS");
    m2::initialize(&map)
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    serialln!("PANIC: {}", info);
    machine::exit(false)
}
