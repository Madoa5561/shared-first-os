//! Single-CPU privilege transitions. Assembly saves a complete user context,
//! reloads kernel CR3, then returns to the suspended kernel caller. No Rust code
//! runs on user stacks; no user pointer is followed by the syscall dispatcher.
use core::{
    arch::{asm, global_asm},
    mem::{offset_of, size_of},
    ptr,
};

// SAFETY: user_entry.S follows Context's asserted offsets and the SysV callee
// save convention. Context and kernel stack remain supervisor-mapped in every
// root. Only one invocation may be active; IF=0 and no APs. CPL0 traps are fatal.
global_asm!(include_str!("user_entry.S"));

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Context {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub vector: u64,
    pub error: u64,
    pub rip: u64,
    pub cs: u64,
    pub flags: u64,
    pub rsp: u64,
    pub ss: u64,
}
const _: () = assert!(size_of::<Context>() == 176 && offset_of!(Context, cs) == 144);

impl Context {
    pub fn new(entry: u64, stack: u64) -> Self {
        Self {
            rip: entry,
            rsp: stack,
            cs: 0x23,
            ss: 0x1b,
            flags: 2,
            ..Self::default()
        }
    }
    pub fn valid_return(&self) -> bool {
        self.cs == 0x23 && self.ss == 0x1b && self.rip < (1 << 47) && self.rsp < (1 << 47)
    }
}

#[repr(C, packed)]
struct Tss {
    reserved: u32,
    rsp: [u64; 3],
    reserved2: u64,
    ist: [u64; 7],
    reserved3: u64,
    reserved4: u16,
    iomap: u16,
}
#[repr(C, packed)]
struct Descriptor {
    limit: u16,
    base: u64,
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
#[repr(align(16))]
struct Stack([u8; 16384]);
static mut ENTRY_STACK: Stack = Stack([0; 16384]);
static mut FAULT_STACK: Stack = Stack([0; 16384]);
static mut TSS: Tss = Tss {
    reserved: 0,
    rsp: [0; 3],
    reserved2: 0,
    ist: [0; 7],
    reserved3: 0,
    reserved4: 0,
    iomap: 104,
};
static mut GDT: [u64; 7] = [0; 7];
static mut IDT: [Gate; 256] = [EMPTY; 256];
const _: () = assert!(size_of::<Tss>() == 104);

unsafe extern "C" {
    fn m4_enter(context: *mut Context, root: u64, kernel: u64);
    static m4_vectors: [u64; 32];
    fn m4_vector_128();
    fn m4_vector_255();
    static m4_fault_address: u64;
    static m4_observed_root: u64;
}

pub fn init() {
    // SAFETY: called once after M3 with interrupts off, on a healthy kernel
    // stack. All descriptor/stack storage is static, supervisor-only and mapped
    // in every root. The I/O bitmap is beyond the TSS limit, denying user I/O.
    unsafe {
        let tss = ptr::addr_of_mut!(TSS);
        ptr::addr_of_mut!((*tss).rsp)
            .cast::<u64>()
            .write_unaligned(ptr::addr_of!(ENTRY_STACK.0) as u64 + 16384);
        ptr::addr_of_mut!((*tss).ist)
            .cast::<u64>()
            .write_unaligned(ptr::addr_of!(FAULT_STACK.0) as u64 + 16384);
        let base = tss as u64;
        ptr::addr_of_mut!(GDT).write([
            0,
            0x00af9a000000ffff,
            0x00cf92000000ffff,
            0x00cff2000000ffff,
            0x00affa000000ffff,
            103 | ((base & 0xffffff) << 16) | (0x89 << 40) | (((base >> 24) & 255) << 56),
            base >> 32,
        ]);
        let gdtr = Descriptor {
            limit: (size_of::<[u64; 7]>() - 1) as u16,
            base: ptr::addr_of!(GDT) as u64,
        };
        asm!("lgdt [{}]", in(reg) ptr::addr_of!(gdtr), options(readonly, nostack));
        asm!("push 8", "lea rax, [rip + 2f]", "push rax", "retfq", "2:",
            "mov ax, 0x10", "mov ds, ax", "mov es, ax", "mov ss, ax",
            "xor eax, eax", "mov fs, ax", "mov gs, ax", "mov ax, 0x28", "ltr ax",
            out("rax") _);
        for index in 0..256 {
            let fallback = if index == 128 {
                m4_vector_128 as *const () as u64
            } else {
                m4_vector_255 as *const () as u64
            };
            let address = m4_vectors.get(index).copied().unwrap_or(fallback);
            ptr::addr_of_mut!(IDT)
                .cast::<Gate>()
                .add(index)
                .write(Gate {
                    low: address as u16,
                    selector: 8,
                    ist: if index == 8 { 1 } else { 0 },
                    attributes: if index == 128 { 0xee } else { 0x8e },
                    middle: (address >> 16) as u16,
                    high: (address >> 32) as u32,
                    reserved: 0,
                });
        }
        let idtr = Descriptor {
            limit: (size_of::<[Gate; 256]>() - 1) as u16,
            base: ptr::addr_of!(IDT) as u64,
        };
        asm!("lidt [{}]", in(reg) ptr::addr_of!(idtr), options(readonly, nostack));
        // No FPU context switching yet. Deny user x87/MMX/SIMD via #NM.
        let mut cr0: u64;
        asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack));
        asm!("mov cr0, {}", in(reg) cr0 | (1 << 3), options(nostack));
    }
}

/// Both roots retain the current kernel stack/code/descriptors. No other task,
/// CPU or callback may execute this transition concurrently.
pub unsafe fn enter(context: &mut Context, root: u64, kernel: u64) -> (u64, u64) {
    assert!(context.valid_return());
    context.flags = (context.flags & 0xcd5) | 2; // No IF, IOPL, NT, VM, TF or AC.
    // SAFETY: caller owns the roots and Context, with all kernel mappings live.
    // The assembly saves registers into Context and returns only under kernel CR3.
    unsafe {
        m4_enter(context, root, kernel);
        (
            ptr::addr_of!(m4_fault_address).read(),
            ptr::addr_of!(m4_observed_root).read(),
        )
    }
}

pub fn current_root() -> u64 {
    let root;
    // SAFETY: read-only control-register observation in ring 0.
    unsafe {
        asm!("mov {}, cr3", out(reg) root, options(nomem, nostack));
    }
    root
}

// SAFETY: unique assembly diagnostic target; arguments follow SysV, never returns.
#[unsafe(no_mangle)]
extern "C" fn m4_fatal(vector: u64, error: u64, rip: u64) -> ! {
    panic!("M4 kernel exception: vector={vector} error={error:#x} rip={rip:#x}")
}
