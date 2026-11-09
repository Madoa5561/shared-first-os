use core::arch::asm;

pub fn disable_interrupts() {
    // SAFETY: only called in ring 0 under QEMU. No interrupt scheduler is enabled.
    unsafe {
        asm!("cli", options(nomem, nostack));
    }
}

pub fn halt() -> ! {
    loop {
        // SAFETY: ring 0 only; external interrupt handlers are not installed.
        unsafe {
            asm!("cli", "hlt", options(nomem, nostack));
        }
    }
}

/// Install a complete four-level tree. All current code, data, stack, GDT and
/// IDT mappings must remain valid. Caller owns root and all subordinate frames.
pub unsafe fn activate_paging(root: u64) {
    // SAFETY: caller preserves the live mappings. This runs only on the boot
    // CPU, in ring 0 with interrupts off; no PCID/LA57 or other CPUs are used.
    unsafe {
        let features = core::arch::x86_64::__cpuid(0x80000001);
        assert_ne!(features.edx & (1 << 20), 0, "NX is required");
        let mut cr4: u64;
        asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack));
        assert_eq!(cr4 & ((1 << 12) | (1 << 17)), 0, "LA57/PCID unsupported");
        // Clearing PGE flushes any global translations left by the loader.
        cr4 &= !(1 << 7);
        asm!("mov cr4, {}", in(reg) cr4, options(nostack));
        let low: u32;
        let high: u32;
        asm!("rdmsr", in("ecx") 0xc0000080_u32, out("eax") low, out("edx") high, options(nomem, nostack));
        asm!("wrmsr", in("ecx") 0xc0000080_u32, in("eax") low | (1 << 11), in("edx") high, options(nostack));
        let mut cr0: u64;
        asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack));
        cr0 |= 1 << 16; // Supervisor writes must respect read-only PTEs.
        asm!("mov cr0, {}", in(reg) cr0, options(nostack));
        asm!("mov cr3, {}", in(reg) root, options(nostack));
    }
}

pub fn invalidate_page(address: u64) {
    // SAFETY: ring 0, canonical address from the page-table engine, one CPU.
    // Memory clobber orders PTE writes before invalidation and frame reclamation.
    unsafe {
        asm!("invlpg [{}]", in(reg) address, options(nostack));
    }
}

pub fn stack_pointer() -> u64 {
    let stack;
    // SAFETY: register-only observation, no access to stack contents.
    unsafe {
        asm!("mov {}, rsp", out(reg) stack, options(nomem, nostack));
    }
    stack
}

/// Switch to an exclusively owned, mapped RW/NX stack and never return.
pub unsafe fn switch_stack(top: u64, entry: extern "C" fn() -> !) -> ! {
    // SAFETY: caller provides an aligned stack with sufficient space; entry
    // follows SysV and never returns. No values on the old stack remain borrowed.
    unsafe {
        asm!("mov rsp, rdi", "xor rbp, rbp", "call rsi", "ud2",
            in("rdi") top, in("rsi") entry, options(noreturn));
    }
}

pub fn exit(success: bool) -> ! {
    let value: u32 = if success { 0x10 } else { 0x11 };
    // SAFETY: the supported QEMU command reserves port 0xf4 for isa-debug-exit.
    // Only our boot CPU uses it. No host physical hardware is an execution target.
    unsafe {
        asm!("out dx, eax", in("dx") 0xf4_u16, in("eax") value, options(nomem, nostack));
    }
    halt()
}
