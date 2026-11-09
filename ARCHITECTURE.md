# Architecture: Object-Centric Memory (OCM)

> このファイルは初期設計の保存版です。M1以降の実装状況と設計更新は
> [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) を参照してください。

## Objective
Evaluate whether treating shared, immutable or copy-on-write memory objects as first-class kernel resources can reduce *resident physical memory usage* and copies for particular multi-task workloads without degrading isolation, latency or reliability.

This is not the false premise that OSes do not already share libraries, mmap file pages, or COW frames. Distinguish genuinely new behaviors, integration benefits and benchmark workloads.

## Layers

1. **Boot and hardware:** Limine boot information; serial output; paging; interrupts; QEMU.
2. **Frame and address-space management:** protected physical frame allocator; page tables; translation/permissions; memory map reserved frame exclusions.
3. **Capability kernel:** task-local handle table; object rights (`READ`, `WRITE`, `MAP`, `SHARE` as needed); generation-safe lookup; grant/delegation; explicit lifecycle and revocation.
4. **Memory objects:** kernel-internal ObjectId; size, kind, physical-frame backing, immutable/sealed flag, COW state, mapping/refcount information; separate metadata from payload.
5. **Task/user API:** minimal ring-3 task sandbox with syscalls for object creation, grant, mapping, clone-on-write, release; demonstrate sharing without duplicate payload frames.
6. **Policy/optimization:** read-mostly sharing, optional compression/deduplication, observed usage, and baseline comparisons.

## Critical security invariant
A capability is an authorization to *request* a mapping. Page tables (and eventually another real protection mechanism) must enforce access on every CPU read/write. A raw numerical ID must never confer rights by itself. Stale handle + reused slot must not access a newly allocated object.

## Stage-1 model (hybrid by design)
Untrusted native processes have separate address spaces and share physical pages by mapping the same backing frames at potentially different virtual addresses with independently checked rights. This tests object-centric policy while retaining hardware-enforced protection.

Benefits: implementable on commodity x86-64, debuggable, comparable to baseline mmap/COW. Limitations: still requires page tables and TLB management; control flow and native raw pointers remain process-local.

## Experimental Stage-2 alternatives
- **Single address space with verified/sandboxed code:** language runtime, restricted instruction set and/or WASM-like validation; needs defense against native unsafe code and device DMA.
- **Capability hardware:** CHERI-inspired extensions, not assumed present on ordinary x86-64 machines.
- **Hybrid:** shared region mapped at common virtual addresses with task permissions, retaining per-task page-table roots.

Document security assumptions, performance consequences and hardware feasibility before choosing one. A plain shared virtual address space plus software handle checks is NOT an acceptable secure isolation implementation for arbitrary native code.

## Initial object lifecycle
`create -> capability -> map -> share/grant -> map in another task -> optionally clone COW -> unmap -> revoke/release -> reclaim`.

Questions requiring clear answers: How does revoke handle existing mappings? What happens to active readers and writers? How are COW faults handled? When may physical frames be freed? How do permissions and counters behave under concurrency?

## Demonstrations
1. Two isolated tasks map a 4 MiB immutable object backed by identical physical frames; second mapping does not add a second 4 MiB of payload frames.
2. A writer requests COW; changing one 4 KiB page incurs one page copy rather than duplicating all 4 MiB (subject to page granularity).
3. Unauthorized mapping fails; read-only mapping cannot be written; stale handles are rejected; proper unmapping/revocation prevents access.
4. Metadata overhead and mapping/TLB cost are included in the physical memory and performance measurements.

## Baselines
Compare separate anonymous allocations, ordinary shared mappings and COW where implemented. Use identical workloads for throughput/latency/peak frames. Do not compare against an artificially inefficient baseline alone.

## Metrics
- Allocated physical frames/peak frames (exclude free cache only with clear accounting)
- Payload-sharing ratio and metadata overhead
- Copies and page faults
- Create/map/unmap latency, COW fault latency
- Throughput under 1, 2, 4, 8 simulated tasks
- CPU cost and tail latency where measurements permit

## Historical perspective
Microsoft Research Singularity explored software-isolated processes with language guarantees; it is not evidence that user objects can be indiscriminately shared safely. seL4 offers capability-mediated memory sharing while retaining virtual-memory mappings. Position this project honestly relative to predecessors.
