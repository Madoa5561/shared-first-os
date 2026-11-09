You are the principal kernel engineer for this repository. Build an original Rust research operating system called "Shared-First OS" that explores object-centric, capability-controlled shared physical memory and challenges the assumption that process-scoped allocation is the ideal abstraction for all workloads.

First read `AGENTS.md`, `README.md`, `docs/ARCHITECTURE.md`, and `docs/ROADMAP.md`. Treat their security invariants and milestone acceptance gates as requirements.

DO NOT merely produce a design proposal. Begin implementation immediately with **M1 only**:
1. Inspect available toolchain and check current compatibility of chosen pinned Rust, Limine and QEMU components.
2. Implement an actual bootable x86-64 Rust `no_std` kernel and Limine boot image.
3. Print `Shared-First OS booted` plus usable/reserved physical memory map summary to a QEMU serial console.
4. Provide reproducible build, headless QEMU run and automated smoke-test commands that detect the marker and exit predictably. Include Windows 11 + WSL2 setup notes as needed.
5. Execute the build and QEMU boot test if the environment supports it. Fix build/boot errors and show actual command, exit code and captured console output. If unavailable, state precise unverified gates, not guessed success.
6. Update documentation, include a short technical decision log and report files changed, verified tests, and exact next steps for M2.

Constraints: never install to physical disks; do not bypass hardware isolation for untrusted native tasks; do not implement fake mocks as proof that a bootable OS exists; avoid over-engineering a GUI, filesystem or network stack; do not pretend existing OSes lack mmap, shared pages or COW. Later milestones will implement secure object capabilities, sharing, COW and fair benchmarks.

Choose a modest, maintainable implementation that leaves the repository working at the end of the task.
