# Roadmap and gates

> このファイルは初期ロードマップの保存版です。現在の達成状況は
> [docs/ROADMAP.md](docs/ROADMAP.md) を参照してください。

## M1: Bootable kernel (do this first)
- Confirm host prerequisites and pick compatible pinned Rust / Limine / QEMU versions.
- Rust `no_std` + explicit target/linker config + Limine boot image.
- Boot under QEMU; log `Shared-First OS booted` over COM1 serial.
- Parse and display firmware memory map, usable/reserved region totals; never treat reserved regions as allocator inputs.
- `make run` (or documented equivalent) and `make test-boot` with timeout, serial marker assertion and deterministic exit mechanism.
- Include exact Windows+WSL2/Ubuntu setup instructions if on Windows.
- Gate: real QEMU serial transcript and exit code; if not run, mark UNVERIFIED.

## M2: Physical memory & paging
- Bitmap/buddy allocator of usable physical frames, correct reserved/bootloader treatment.
- Page-table mapping/unmapping, W^X where practical, kernel stack guard where feasible, TLB handling.
- Host unit tests for allocator and failure cases, QEMU allocation/reclaim checks.
- Gate: no double allocation; no reserved frame use; deterministic tests.

## M3: Capability object store
- Object descriptor, frame backing, reference accounting, permission system and per-task handles with generations.
- Immutable shared payload, authority-checked map/release/revoke APIs.
- Unit tests covering stale handles, rights downgrade, double free, overflow and partial failure cleanup.
- Gate: model-backed tests and kernel integration of object store.

## M4: Ring-3 tasks and true sharing
- Minimal task switch / user transition, syscall boundary, per-task address spaces.
- Shared physical pages mapped read-only to two real isolated tasks.
- COW page-fault handling and writable mappings controlled by rights.
- Gate: QEMU demo proving physical-frame identity, isolation, COW and clean reclamation. No fake user-task simulator as substitute.

## M5: Adaptive policies
- Optional read-mostly object interning/deduplication, pressure-triggered compression; correct metadata accounting.
- Explicit overhead and latency analysis; avoid compressing already compressed/random data when harmful.
- Gate: enabled/disabled A/B tests on identical workload.

## M6: Baselines and benchmark report
- Implement fair baseline modes for anonymous/private/shared/COW where relevant.
- Report peak physical frames, page faults, COW copies, metadata, throughput and latency.
- Gate: reproducible script, raw results, sample sizes, correct interpretation of where results generalize.

## M7: New isolation paradigm (research stage)
- Compare language-isolated single address space, common mappings with paging, and capability-hardware-inspired approaches.
- Prototype only with explicit threat model and proof/test plan; cannot assert arbitrary native-code isolation from handle checking alone.
- Gate: attack-oriented tests and measured memory savings relative to hybrid M4/M6 architecture.

## Quality gates across milestones
- Formatting, lint where available and host tests for portable components.
- No `unsafe` without stated invariants.
- No successful test claims without stdout/stderr and exit status.
- Maintain design decision log and run instructions.
