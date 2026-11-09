# Shared-First OS

**A bootable Rust research kernel for capability-controlled shared memory.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Research prototype](https://img.shields.io/badge/status-research%20prototype-orange)](docs/ROADMAP.md)

Shared-First OS explores immutable memory objects and copy-on-write across isolated
tasks. It runs a real `no_std` x86-64 kernel under QEMU, with hardware paging,
task-local capabilities, and reproducible tests backed by serial logs.

This is an early research preview. The current system executes finite built-in
workloads and exits automatically. There is no desktop, interactive shell,
filesystem, network stack, general application loader, or physical-machine installer.

**[Download the research preview](https://github.com/Madoa5561/shared-first-os/releases/tag/v0.1.0)**
· **[Build instructions](docs/BUILD.md)**
· **[Architecture](docs/ARCHITECTURE.md)**
· **[Verification report](docs/M8-VERIFICATION.md)**

## What works

- **UEFI boot:** pinned Limine bootloader, serial diagnostics, parsed memory map.
- **Memory management:** usable-frame allocator, peak tracking, four-level page
  tables, W^X protections, guarded kernel stack, explicit TLB handling.
- **Capabilities:** per-task slot + generation handles, rights attenuation,
  immutable objects, mapping, release, and revocation of existing mappings.
- **Real userspace:** CPL3 tasks with independent page-table roots and an
  `int 0x80` boundary. Two tasks share the same 4 MiB of physical backing.
- **Copy-on-write:** an authorized write fault copies only the affected 4 KiB
  page, preserving the reader's data and reclaiming copies on teardown.
- **Optional policies:** bounded deduplication and pressure-triggered compression
  experiments, including metadata accounting and incompressible-data tests.
- **Comparisons:** private, ordinary shared, ordinary COW, object shared, and
  object COW modes with saved samples and descriptive statistics.
- **Restricted interpreter:** eight validated bytecode instructions, private
  scratch memory, checked object reads, persistent fuel limits, and attack tests.
- **Bounded bytecode scheduling:** four fixed task slots, instruction quanta,
  round-robin dispatch, cumulative fuel quotas, and task-local cleanup.

Native tasks retain hardware isolation. The restricted interpreter is trusted
kernel code; it accepts a small custom language, not arbitrary native code or WASM.

## Try the prebuilt image

Download `shared-first-os-v0.1.0-preview.zip` from the
[v0.1.0 release](https://github.com/Madoa5561/shared-first-os/releases/tag/v0.1.0).
The ZIP contains the bootable ISO, kernel ELF, checksums, and license notices.
The ISO is also available as a separate asset.

Run it in QEMU with OVMF. The tested environment is QEMU 10.2.1, q35, qemu64,
TCG, one CPU, and 128 or 256 MiB RAM. The following command is for Linux/WSL
with Ubuntu's OVMF paths; see [BUILD.md](docs/BUILD.md) for exact package versions.

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

Expect `Shared-First OS booted`, followed by `M1 PASS` through `M7 PASS`.
The full run takes about 68 seconds in the recorded environment. QEMU exits with
**code 33 on success** through `isa-debug-exit`; this nonzero code is intentional.
Code 35 indicates a kernel panic. Use QEMU only; physical-machine boot is untested.

## Build from source

The verified build path uses Windows PowerShell 7, Rust 1.96.0, and WSL2 Ubuntu
26.04. The kernel has no external Rust crate dependencies. Setup commands are
explicit; normal build scripts do not install host packages.

```powershell
git clone https://github.com/Madoa5561/shared-first-os.git
cd shared-first-os

# First follow docs/BUILD.md to prepare Rust, WSL, QEMU, OVMF and xorriso.
./tools/fetch-limine.ps1       # Fetch and verify the pinned bootloader
./tools/project.ps1 build     # Build ELF and UEFI ISO
./tools/project.ps1 test-boot # Boot, validate evidence and exit
./tools/project.ps1 verify    # All static, unit, boot and failure-path checks
```

Outputs:

| Path | Contents |
| --- | --- |
| `build/normal/shared-first.iso` | Normal bootable UEFI image |
| `target/x86_64-unknown-none/release/shared-first-kernel` | Rust kernel ELF |
| `artifacts/commands-*/` | Exact commands, exit status, stdout/stderr |
| `artifacts/boot-*/` | Serial logs, machine-readable results, benchmark tables |

The scripts enforce locked versions. An ISO's timestamp metadata is not fixed,
so byte-for-byte reproducible ISO builds are not claimed.

## Verified results

The M8 working tree passed **49 Rust tests**, **42 Python validator tests**, format
and Clippy checks, normal QEMU boots at **128 and 256 MiB**, and the intentional
panic, unexpected page fault and timeout paths. All 18 verification commands passed.
Each normal boot validates 17 M8 dispatches, non-yielding loop isolation, shared
physical backing, revocation during suspension, admission quotas and cleanup.
See [M8 verification](docs/M8-VERIFICATION.md) and [raw evidence](docs/evidence/m8).
These checks are local Windows/WSL results; a new GitHub CI run is not claimed.

The M7 checkpoint passed **43 Rust tests**, **38 Python validator tests**, format
and Clippy checks, normal QEMU boots at **128 and 256 MiB**, intentional panic,
unexpected page fault, and timeout cleanup. All 18 verification commands passed.
CI checks portable code and validators; QEMU evidence comes from the recorded
Windows/WSL runs. Full logs and hashes are in [docs/evidence/m7](docs/evidence/m7).

| Experiment | Observed result | Interpretation |
| --- | --- | --- |
| Two CPL3 tasks read a 4 MiB object | Same 1,024 physical payload pages | True sharing, with independent address spaces |
| Writer changes one page | One private 4 KiB COW copy | Reader retains the original data |
| M6 ordinary vs. object sharing/COW | Equal payload frame counts | Existing shared-memory techniques provide the same payload savings |
| M7 restricted bytecode vs. native control | 19 fewer pool frames (77,824 bytes) | Limited read workload; native control includes an unused fixture page |
| M7 checkpoint vs. M6 checkpoint | Larger kernel and retained bootloader regions | Overall boot-time free RAM did not improve |

Benchmark times are **QEMU guest TSC ticks**, not calibrated seconds or physical
CPU cycles. M7 bulk host reads and native byte loops use different execution
mechanisms. These results do not establish general application speedups or
superiority over Linux, seL4, WASM runtimes, or other operating systems.

## Current boundaries

- One CPU, interrupts disabled, finite workloads; no native preemption/SMP.
  Bytecode can be suspended at instruction boundaries; host reads remain synchronous.
- Usable physical memory must end at or below 1 GiB.
- Fixed-capacity tables and built-in native/bytecode fixtures.
- Compression and deduplication are opt-in experiments in a small policy domain.
- The interpreter and its host adapter are part of the trusted computing base.
- Physical hardware, KVM, Secure Boot, DMA isolation, side channels, and formal
  security proofs are outside the verified scope.

## Project map

| Location | Purpose |
| --- | --- |
| `src/` | Kernel, portable memory/capability logic, restricted interpreter |
| `tools/` | Pinned build, QEMU runner, evidence validators and tests |
| `docs/ARCHITECTURE.md` | Invariants, permission/lifetime rules and implementation |
| `docs/ROADMAP.md` | Milestones and remaining research questions |
| `docs/M7-THREAT-MODEL.md` | Restricted-language trust boundary and test plan |
| `docs/M1-VERIFICATION.md` … `docs/M7-VERIFICATION.md` | Checkpoint reports |
| `docs/evidence/` | Historical raw results, commands and source hashes |

Detailed engineering documents are currently written in Japanese. Historical
evidence includes the original machine's local paths; those paths describe the
recorded environment and are not prerequisites for building your checkout.
M1–M8 are complete within their documented scope. M8 adds bounded bytecode
scheduling; see its [design and limits](docs/M8-DESIGN.md). The downloadable
v0.1.0 release remains the M7 checkpoint; build the working source to test M8.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Small changes with a clear invariant,
reproducible commands, and honest verification evidence are welcome. Preserve
hardware isolation for native code, and include metadata and mapping costs in
memory claims. Report security concerns using [SECURITY.md](SECURITY.md).

## License and acknowledgements

Original project code is available under the [MIT License](LICENSE).
Limine-derived ABI definitions and bootloader binaries retain their respective
upstream terms; see [THIRD_PARTY.md](THIRD_PARTY.md) and
[LICENSES/Limine.txt](LICENSES/Limine.txt). QEMU/OVMF are external execution tools
and are not bundled in the release.

Related work: [seL4 shared-memory mapping](https://docs.sel4.systems/Tutorials/mapping.html),
[Singularity](https://www.microsoft.com/en-us/research/project/singularity/),
[WebAssembly's security model](https://webassembly.org/docs/security/), and
[CHERI](https://www.cl.cam.ac.uk/research/security/ctsrd/cheri/).
