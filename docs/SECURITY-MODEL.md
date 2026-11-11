# Security model

Shared-First OS is an experimental kernel. Its tests exercise specific isolation
properties; they do not establish production security or a formal proof.

## Trust boundary

Native task fixtures execute at CPL3 with independent page tables. The kernel
derives their identities at the syscall boundary. After mapping, access is
enforced by PTE permissions. Revocation unmaps applicable views and invalidates
translations on the single CPU.

Restricted bytecode is interpreted as data by trusted kernel code. An attacker
may choose every code byte, constant and branch target, and know other tasks'
handle values. It cannot select the runtime task identity, modify the capability
tables or obtain a Rust reference through the language. The interpreter has no
separate hardware isolation from the kernel.

The compiler, kernel, interpreter, parser, object store, physical backend and
boot environment are trusted. Bootloader metadata and pointers are trusted;
external file contents are validated. Module order and the compiled READ manifest
are launch policy, not signatures or authenticated code identity.

## Enforced rules

- Every instruction, including unreachable code, is validated before admission.
- Registers contain integers; the language has no native pointer or JIT operation.
- Each task has private, bounds-checked scratch memory.
- Object reads check task-local slot, generation, READ permission and byte bounds.
- Rights can be attenuated, not expanded by a recipient.
- Per-task fuel is never replenished by yielding or cancellation. Dispatch limits
  add a separate bound on scheduling turns.
- Existing native mappings must be removed when applicable authority is revoked.
- Ordinary scope exit stops each live task once and releases acquired references.

A copied numerical handle is not authority in another task. If that number is
already valid in the receiving task, it denotes that task's own table entry.
Revocation denies future reads; it cannot erase values already read or returned.

## Tested scope

Tests cover malformed packages and instructions, bounds, stale/foreign handles,
rights attenuation, read-only and NX faults, COW, revocation during suspension,
fuel exhaustion, launch rejection and resource cleanup. Generated cases compare
runtime results with a separate execution model on both the host and QEMU.
See [verification](VERIFICATION.md) for counts and exact evidence.

The generated corpus is finite and deterministic. It is not exhaustive fuzzing,
model checking of all programs, or a proof against runtime implementation bugs.
Host-only allocation/seal failure injection does not prove hardware failure recovery.

## Outside the verified scope

SMP concurrency, native preemption, DMA/IOMMU protection, cache and timing side
channels, malicious trusted code, physical hardware, Secure Boot and formal proofs
are not covered. Host calls are synchronous, so instruction budgets do not imply
real-time latency guarantees. Cleanup is not guaranteed after abort, power loss
or deliberate forgetting of owned values. Logical clearing is not secure erasure.

Report suspected vulnerabilities using [the security reporting policy](../SECURITY.md).
