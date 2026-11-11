# Shared-First OS

**An open-source research operating system exploring capability-controlled shared memory.**

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Checks](https://github.com/Madoa5561/shared-first-os/actions/workflows/check.yml/badge.svg)](https://github.com/Madoa5561/shared-first-os/actions/workflows/check.yml)

Shared-First OS is a small Rust `no_std` kernel for x86-64. It investigates
object-centric memory ownership, immutable sharing and copy-on-write while
preserving hardware isolation for native tasks.

The project runs reproducible experiments in QEMU and publishes their source,
serial logs and resource accounting. It is an experimental research system:
the image boots, runs finite workloads and exits. It has no desktop, interactive
shell, filesystem, network stack or physical-machine installer.

[Download the research preview](https://github.com/Madoa5561/shared-first-os/releases/tag/v0.2.0)
· [Build instructions](docs/BUILD.md)
· [Architecture](docs/ARCHITECTURE.md)
· [Verification](docs/VERIFICATION.md)

## Research focus

- How should shared objects and task-local authority interact with memory lifetime?
- What are the physical payload, metadata and mapping costs of sharing and COW?
- Which costs change when a restricted interpreter executes a bounded workload
  instead of native tasks with separate address spaces?

Ordinary shared memory and COW already provide payload savings. This project
does not claim to invent those techniques or outperform general-purpose systems.
Measurements separate actual physical frames from virtual mappings and metadata.

## Implemented capabilities

- UEFI boot through a pinned Limine build, serial diagnostics and memory-map parsing.
- Physical frame allocation, protected page tables, W^X checks and guarded stacks.
- Task-local capabilities with generations, rights attenuation and mapping revocation.
- Native CPL3 tasks with independent page-table roots and shared physical backing.
- Immutable objects and page-granular copy-on-write with teardown accounting.
- Experimental deduplication and compression policies with enabled/disabled comparisons.
- A restricted bytecode interpreter with validation, private scratch space and fuel limits.
- External bytecode packages, bounded scheduling, scoped execution and trusted READ grants.
- Revocation, cancellation, execution snapshots and a bounded launch API that returns
  startup errors or task results after reclaiming acquired resources.
- Generated launch cases checked against independent Rust and Python reference models.

The interpreter is trusted kernel code. It accepts a small custom language,
not arbitrary native code or WebAssembly.

## Run a preview

Download the ISO or ZIP from the [releases](https://github.com/Madoa5561/shared-first-os/releases).
The ZIP includes the kernel ELF, external program modules, examples, checksums
and license notices. Verify the files with `SHA256SUMS.txt`.

The tested setup uses QEMU 10.2.1, OVMF, q35, TCG, one CPU and 128 or 256 MiB RAM.
On Linux/WSL with Ubuntu's OVMF paths:

```sh
cp /usr/share/OVMF/OVMF_VARS_4M.fd ./OVMF_VARS.fd
qemu-system-x86_64 \
  -machine q35 -accel tcg -cpu qemu64 -smp 1 -m 128 \
  -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
  -drive if=pflash,format=raw,file=./OVMF_VARS.fd \
  -cdrom shared-first.iso -boot order=d \
  -display none -serial stdio -monitor none -nic none -no-reboot \
  -device isa-debug-exit,iobase=0xf4,iosize=0x04
```

The serial console starts with `Shared-First OS booted` and prints the experiment
results. **QEMU exit code 33 means success**; code 35 indicates a kernel panic.
The test runner allows 180 seconds per normal boot. Individual PASS markers
alone do not establish success: the runner also checks results and resource accounting.

## Build and test

The verified build path uses PowerShell 7, Rust 1.96.0 and WSL2 Ubuntu 26.04.
There are no external Rust crate dependencies. Follow [the setup guide](docs/BUILD.md)
before running these commands:

```powershell
git clone https://github.com/Madoa5561/shared-first-os.git
cd shared-first-os
./tools/fetch-limine.ps1
./tools/project.ps1 check
./tools/project.ps1 build
./tools/project.ps1 test-boot
./tools/project.ps1 verify
```

The normal ISO is written to `build/normal/shared-first.iso`. Exact commands,
serial logs and machine-readable results are saved under `artifacts/`.
Tool versions and bootloader hashes are pinned; ISO timestamp metadata is not,
so byte-for-byte identical ISO builds are not claimed.

## Evidence and limits

[The verification report](docs/VERIFICATION.md) records tested behavior and
unverified scope. QEMU checks run locally on Windows/WSL; GitHub CI runs portable
Rust tests, compile-time borrowing checks, Clippy and Python validator tests.

The native sharing experiment checks the same 1,024 physical pages in two
isolated tasks. A write to one COW page produces one private 4 KiB copy.
These are controlled workloads, not general application performance claims.
Benchmark values are guest TSC ticks, not calibrated CPU time.

Execution uses one CPU with interrupts disabled. Tables and workloads are bounded;
there is no native preemption, SMP, general native application loader or dynamic
task-slot reuse. The kernel manifest and boot module order are trusted.
Revocation blocks subsequent reads but cannot erase data already read.
Cleanup on normal scope exit does not cover aborts, power loss or deliberate
`mem::forget`. Physical hardware, KVM, Secure Boot, DMA isolation, side-channel
resistance and formal security proofs are outside the verified scope.

## Documentation

- [Architecture and ownership](docs/ARCHITECTURE.md)
- [Bytecode format and packaging](docs/BYTECODE.md)
- [Security model](docs/SECURITY-MODEL.md)
- [Build and test environment](docs/BUILD.md)
- [Verification and raw evidence](docs/VERIFICATION.md)
- [Contributing](CONTRIBUTING.md) · [Security reporting](SECURITY.md)

## License

Original code is licensed under the [MIT License](LICENSE). Limine-derived
definitions and bundled bootloader files retain their upstream terms; see
[third-party notices](THIRD_PARTY.md) and [the Limine license](LICENSES/Limine.txt).
QEMU and OVMF are external tools and are not bundled.
