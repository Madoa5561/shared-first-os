# Codex project instructions

## Mission
Build a bootable and testable **research operating system** examining object-centric, capability-controlled shared memory. Produce running code and evidence, not just conceptual explanations. Follow `docs/ROADMAP.md` in order, one working milestone at a time.

## Non-negotiables
- Write real Rust `no_std` kernel code for x86-64. Use QEMU as the only execution target until explicitly authorized otherwise.
- Prefer a small, auditable kernel: unsafe code is allowed only at necessary hardware/memory boundaries and must be documented with safety invariants.
- NEVER remove memory isolation simply to make shared memory faster. Privilege separation via hardware paging stays in place for native untrusted user tasks in the first architecture.
- Capabilities must be unforgeable across task boundaries (per-task table index + generation checked in kernel, not a globally predictable token alone). Prevent stale-handle reuse.
- Enforcement of read/write/execute rights must match page-table permissions. Writable alias creation, lifetime, copy-on-write, map/unmap, revocation and concurrent access require explicit design.
- Memory mapped to different tasks must be backed by the **same physical pages** when advertised as shared; prove it in tests instead of merely passing copies over IPC.
- Account separately for actual physical frames, metadata, process virtual mappings, file-backed pages, and compressed bytes. Do not mistake summed virtual address-space sizes for RAM consumed.
- Separate existing techniques from new experimental ideas; never claim invention or improvement without comparative measurement.
- Do not fake successful builds, QEMU boots, tests, timings or benchmark results. If tools are missing, report exact missing dependencies and what was actually checked.
- No uncontrolled installs, partition modification, or host disk writes. Prefer pinned dependencies and reproducible build scripts.
- Keep `README.md`, `docs/ARCHITECTURE.md`, and `docs/ROADMAP.md` current as real decisions or behavior change.

## Engineering process
1. Inspect existing files and tooling, then select the smallest next milestone with measurable acceptance criteria.
2. Implement that milestone end-to-end, including reproducible commands and a test where feasible.
3. Run format/static checks, host unit tests and QEMU integration tests as appropriate. Preserve serial logs, exact commands and exit status.
4. Report files changed, what passed, what remains unverified, and the next milestone. Leave a buildable intermediate state.
5. Commit milestone checkpoints only where permitted and configured; do not force-push, rewrite history or delete unrelated files.

## Initial milestone
First make a bootable Rust kernel with Limine that prints `Shared-First OS booted` and a parsed usable/reserved memory-map summary to serial output. Include a headless QEMU smoke test that checks the boot marker and terminates predictably. The presence of `cargo check` alone is **not** proof the OS boots.

## Design boundaries
- An object ID is not itself an authorization credential: task-local capabilities are checked on every privileged operation.
- In a conventional x86-64 address space, memory access bypasses the kernel capability lookup *after* mapping. Therefore capability revocation must unmap applicable PTEs (with requisite TLB invalidation) or explicitly document weaker guarantees.
- Use generation counters and refcounts safely; make alias/lifetime rules explicit.
- Leave optional compression, deduplication, and experimental single-address-space isolation for later measured milestones.
