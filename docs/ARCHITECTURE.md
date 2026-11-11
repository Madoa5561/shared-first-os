# Architecture

Shared-First OS is a Rust research kernel for object-centric, capability-controlled
shared memory. Native tasks use hardware paging isolation. Restricted bytecode
runs through a trusted interpreter in the kernel address space.

## Boot and physical memory

A pinned Limine bootloader starts the x86-64 kernel through UEFI/OVMF. The kernel
parses the memory map, reserves non-usable regions and uses a bitmap allocator for
4 KiB frames. Usable memory must end at or below 1 GiB. Bootloader-reclaimable
regions and original module pages remain reserved.

Kernel code is RX, read-only data is R/NX and writable data is RW/NX. HHDM aliases
of executable and read-only kernel regions are protected. Guard pages surround
the kernel stack. Allocation, mapping rollback, unmapping and TLB invalidation
are checked by host tests and real fault probes.

## Native task isolation

Native fixtures execute at CPL3 with independent CR3 roots. An interrupt syscall
boundary derives the calling task identity in the kernel. Native memory access
after mapping is controlled by PTE permissions, not by a software handle lookup
on every load. Revocation removes applicable mappings and invalidates translations.

Two tasks can map the same physical object. Write access to an authorized COW
view copies the affected page, leaving the original reader's data unchanged.
Writable aliases and frame lifetime are enforced at the backend boundary.
The execution target is a single CPU; there is no SMP TLB shootdown.

## Object capabilities

Object identity alone grants no authority. Each task has a capability table;
handles encode a slot and generation, and operations validate task identity,
generation and rights. Generations do not wrap into reusable stale authority.
Rights attenuation cannot grant more than the source capability permits.

Objects own physical backing. References, views and private COW pages are
accounted separately. A final release destroys the backing after applicable
mappings have been removed. Partial allocation and mapping failures unwind
only the resources acquired by that operation.

## Restricted execution

The interpreter accepts eight validated instructions, four integer registers
and 64 bytes of private scratch memory. It provides no native pointers, JIT,
object writes, dynamic allocation or general I/O. Every object read checks the
task-local capability and bounds before reaching physical backing.

Each task has a lifetime fuel budget. Instructions cost one unit, while SUM costs
one plus its byte length. Insufficient fuel prevents execution of that instruction
and preserves its remaining budget. Yielding does not replenish fuel.
A fixed-capacity round-robin scheduler dispatches one to 64 instructions per turn.
Synchronous host calls are not interrupted.

The package loader validates the complete input before publishing an owned Batch.
Sessions borrow the code immutably and one authority host exclusively. Normal
scope exit cancels remaining tasks once. Task snapshots preserve Ready, Halted,
Faulted and Cancelled states and remaining fuel without exposing a mutable host.
See [the bytecode specification](BYTECODE.md).

## Trusted launch policy

Launch accepts an empty object store and a fixed three-program READ manifest.
A non-executing owner occupies the remaining task slot. Grants place task-local
handles in initial register r1. Module contents do not select rights or task IDs.
The manifest and module order are trusted deployment policy, not authenticated
code identity.

Session revocation acts through its existing host. It removes object references
and backing synchronously while preserving task PC, registers and fuel.
A resumed read fails normal authorization. Already-read values remain available.

The bounded runner combines configuration checks, package validation, launch,
execution, cancellation and teardown. Quantum is 1..64 and the dispatch limit
is 0..1024; zero prepares and then cancels without executing. A result records
startup failure or terminal task states plus before/after resource counts.
Acquired resources end before ordinary return. A busy store is rejected without
destroying pre-existing authority or data.

The dispatch limit bounds turns, not elapsed time. The runner is a synchronous
kernel API, not an interactive job service or a native application loader.

## Experiments and accounting

Optional deduplication compares full immutable page contents before sharing.
Compression operates in a bounded policy domain and accounts for its reserved
arena and metadata. Mapped or private writable pages are excluded as required
by the policy's lifetime rules.

Comparisons include private, ordinary shared, ordinary COW, object shared and
object COW modes. Physical payload frames, private copies, page-table frames,
metadata, virtual mappings and compressed storage are reported separately.
Summed virtual address-space sizes are not physical RAM consumption.

A deterministic launch corpus compares actual execution with a reference model
that does not call the interpreter, scheduler or package parser. A separate Python
model checks the QEMU aggregate. This is finite testing, not a formal proof.

## Verification boundary

The compiler, boot environment, kernel, interpreter, object store and hardware
backend are trusted. Runtime defects are not isolated from the kernel.
Abort, power loss and deliberate forgetting can bypass ordinary Drop cleanup.
Stack high-water usage, physical-machine behavior, DMA isolation, side channels
and SMP concurrency are unverified. See [security assumptions](SECURITY-MODEL.md)
and [recorded verification](VERIFICATION.md).
