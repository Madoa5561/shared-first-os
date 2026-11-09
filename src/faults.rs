//! Minimal ring-0 page-fault handler for controlled M2 probes. Not a general
//! exception subsystem: no TSS/IST or recovery from a real exhausted stack yet.
use core::{
    arch::{asm, global_asm},
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};

static ARMED: AtomicU64 = AtomicU64::new(0);
static EXPECTED_ADDRESS: AtomicU64 = AtomicU64::new(0);
static EXPECTED_ERROR: AtomicU64 = AtomicU64::new(0);
static EXPECTED_IP: AtomicU64 = AtomicU64::new(0);
static COUNT: AtomicU64 = AtomicU64::new(0);

// SAFETY: the assembly follows the x86-64 interrupt frame layout. It preserves
// every modified register, consumes the CPU error code and returns with iretq.
// Only explicitly armed (CR2, error bits, RIP) triples can resume. All other page
// faults enter a non-returning diagnostic with a correctly aligned SysV stack.
global_asm!(
    ".global m2_probe_read",
    "m2_probe_read:",
    ".global m2_read_instruction",
    "m2_read_instruction:",
    "mov rax, [rdi]",
    "xor eax, eax",
    "ret",
    ".global m2_probe_write",
    "m2_probe_write:",
    ".global m2_write_instruction",
    "m2_write_instruction:",
    "mov qword ptr [rdi], 0",
    "xor eax, eax",
    "ret",
    ".global m2_probe_execute",
    "m2_probe_execute:",
    "xor eax, eax",
    "jmp rdi",
    "m2_probe_recover:",
    "mov eax, 1",
    "ret",
    ".global m2_page_fault",
    "m2_page_fault:",
    "push rax",
    "push rcx",
    "push rdx",
    "cmp qword ptr [rip + {armed}], 1",
    "jne 2f",
    "mov rax, cr2",
    "cmp rax, [rip + {address}]",
    "jne 2f",
    "mov rcx, [rsp + 24]",
    "and ecx, 0x1f",
    "cmp rcx, [rip + {error}]",
    "jne 2f",
    "mov rdx, [rsp + 32]",
    "cmp rdx, [rip + {ip}]",
    "jne 2f",
    "mov qword ptr [rip + {armed}], 0",
    "inc qword ptr [rip + {count}]",
    "lea rax, [rip + m2_probe_recover]",
    "mov [rsp + 32], rax",
    "pop rdx",
    "pop rcx",
    "pop rax",
    "add rsp, 8",
    "iretq",
    "2:",
    "mov rdi, cr2",
    "mov rsi, [rsp + 24]",
    "mov rdx, [rsp + 32]",
    "and rsp, -16",
    "call {fatal}",
    "ud2",
    armed = sym ARMED, address = sym EXPECTED_ADDRESS, error = sym EXPECTED_ERROR,
    ip = sym EXPECTED_IP, count = sym COUNT, fatal = sym unexpected_fault,
);

unsafe extern "C" {
    fn m2_page_fault();
    fn m2_probe_read(address: u64) -> u64;
    fn m2_probe_write(address: u64) -> u64;
    fn m2_probe_execute(address: u64) -> u64;
    static m2_read_instruction: u8;
    static m2_write_instruction: u8;
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Gate {
    low: u16,
    selector: u16,
    ist: u8,
    attributes: u8,
    middle: u16,
    high: u32,
    reserved: u32,
}
const EMPTY: Gate = Gate {
    low: 0,
    selector: 0,
    ist: 0,
    attributes: 0,
    middle: 0,
    high: 0,
    reserved: 0,
};
static mut IDT: [Gate; 256] = [EMPTY; 256];
#[repr(C, packed)]
struct Idtr {
    limit: u16,
    base: u64,
}

pub fn init() {
    let selector: u16;
    // SAFETY: single boot CPU with interrupts disabled; IDT is persistent kernel
    // storage and stays mapped after CR3 replacement. Gate uses the current GDT.
    unsafe {
        asm!("mov {0:x}, cs", out(reg) selector, options(nomem, nostack));
        let address = m2_page_fault as *const () as u64;
        ptr::addr_of_mut!(IDT).cast::<Gate>().add(14).write(Gate {
            low: address as u16,
            selector,
            ist: 0,
            attributes: 0x8e,
            middle: (address >> 16) as u16,
            high: (address >> 32) as u32,
            reserved: 0,
        });
        let descriptor = Idtr {
            limit: (core::mem::size_of::<[Gate; 256]>() - 1) as u16,
            base: ptr::addr_of!(IDT) as u64,
        };
        asm!("lidt [{}]", in(reg) ptr::addr_of!(descriptor), options(readonly, nostack));
    }
}

pub enum Probe {
    ReadUnmapped,
    WriteReadOnly,
    ExecuteNx,
}

/// Only probe a controlled M2 test address. For ExecuteNx the byte there must
/// be RET, so an unexpected executable mapping returns safely and fails the test.
pub unsafe fn expect(address: u64, probe: Probe) {
    // SAFETY: forward the caller's controlled-address invariant unchanged.
    unsafe {
        expect_named(address, probe, "M2FAULT");
    }
}

/// Same controlled-address requirements as expect; label distinguishes milestones.
pub unsafe fn expect_named(address: u64, probe: Probe, label: &str) {
    let (instruction, error) = match probe {
        Probe::ReadUnmapped => (ptr::addr_of!(m2_read_instruction) as u64, 0),
        Probe::WriteReadOnly => (ptr::addr_of!(m2_write_instruction) as u64, 3),
        Probe::ExecuteNx => (address, 17),
    };
    let before = count();
    EXPECTED_ADDRESS.store(address, Ordering::SeqCst);
    EXPECTED_ERROR.store(error, Ordering::SeqCst);
    EXPECTED_IP.store(instruction, Ordering::SeqCst);
    ARMED.store(1, Ordering::SeqCst);
    // SAFETY: caller supplies a test address; assembly never creates Rust
    // references to invalid memory. The narrowly armed handler resumes the probe.
    let caught = unsafe {
        match probe {
            Probe::ReadUnmapped => m2_probe_read(address),
            Probe::WriteReadOnly => m2_probe_write(address),
            Probe::ExecuteNx => m2_probe_execute(address),
        }
    };
    ARMED.store(0, Ordering::SeqCst);
    assert_eq!(caught, 1, "expected hardware page fault missing");
    assert_eq!(count(), before + 1);
    crate::serial::print(format_args!(
        "{} address={:#x} error={} recovered=1\n",
        label, address, error
    ));
}

pub fn count() -> u64 {
    COUNT.load(Ordering::SeqCst)
}

/// Test the fatal handler using a known-unmapped test VA and no recovery arm.
pub unsafe fn trigger_unexpected(address: u64) -> ! {
    assert_eq!(ARMED.load(Ordering::SeqCst), 0);
    // SAFETY: caller guarantees this is the controlled, unmapped test address.
    // The assembly access does not construct an invalid Rust reference.
    unsafe {
        m2_probe_read(address);
    }
    panic!("unexpected-fault probe did not fault")
}

extern "C" fn unexpected_fault(address: u64, error: u64, ip: u64) -> ! {
    panic!("unexpected page fault: cr2={address:#x} error={error:#x} rip={ip:#x}");
}
