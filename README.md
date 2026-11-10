# Shared-First OS

**A bootable Rust research kernel for capability-controlled shared memory.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Research prototype](https://img.shields.io/badge/status-research%20prototype-orange)](docs/ROADMAP.md)

Shared-First OS explores immutable memory objects and copy-on-write across isolated
tasks. It runs a real `no_std` x86-64 kernel under QEMU, with hardware paging,
task-local capabilities, and reproducible tests backed by serial logs.

This is an early research preview. The current system executes finite test
workloads, including restricted bytecode loaded from separate boot modules,
and exits automatically. There is no desktop, interactive shell,
filesystem, network stack, general application loader, or physical-machine installer.

**[Download the M12 research preview](https://github.com/Madoa5561/shared-first-os/releases/tag/m12-preview)**
· **[Build instructions](docs/BUILD.md)**
· **[Architecture](docs/ARCHITECTURE.md)**
· **[Verification report](docs/M12-VERIFICATION.md)**

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
- **External bytecode packages:** a versioned boot-module format, full-package
  validation, owned code storage, and explicit code/fuel quotas. No initial
  capabilities are granted by the package itself.
- **Scoped execution:** code and the authority host stay borrowed by one session;
  leaving the scope cancels remaining tasks and releases their object references.
- **Initial read capabilities:** a trusted kernel manifest gives two external
  programs access to the same immutable 4 KiB object. A third program has no grant
  and is denied. Launch arguments carry task-local handles, not global authority.
- **Scoped revocation:** trusted control can revoke a suspended session's object.
  Subsequent reads are denied; restart tests reject old handles while new grants work.

Native tasks retain hardware isolation. The restricted interpreter is trusted
kernel code; it accepts a small custom language, not arbitrary native code or WASM.

## Try the prebuilt image

Download `shared-first-os-m12-preview.zip` from the
[M12 release](https://github.com/Madoa5561/shared-first-os/releases/tag/m12-preview).
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

Expect `Shared-First OS booted`, followed by `M1 PASS` through `M12 PASS`.
The full benchmark suite can take over 90 seconds under TCG; the test runner allows
up to 180 seconds per boot. QEMU exits with
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

M12 adds scoped revocation, partial cancellation and generation-safe restart of
external bytecode. Its verification passed **66 Rust unit tests**, **3 documentation
compile tests**, **57 Python tests**, and all **33 commands** covering QEMU at 128/256 MiB,
seven module variants and failure paths. See [M12 verification](docs/M12-VERIFICATION.md)
and [raw evidence](docs/evidence/m12).

M11 adds trusted initial READ grants, argument delivery and launch cleanup.
The final verification passed **63 Rust unit tests**, **3 documentation compile
tests**, **54 Python tests**, and all **31 commands** covering static checks, QEMU
at 128/256 MiB, six external-input variants, and failure paths. An initial 90-second
timeout is recorded separately as a failure; the final run uses a 180-second limit.
See [M11 verification](docs/M11-VERIFICATION.md) and [raw evidence](docs/evidence/m11).

The M10 verification run includes **58 Rust unit tests**, **3 documentation compile tests**,
and **51 Python tests**. QEMU tests cover 128/256 MiB,
external-module changes and rejection, and four session cleanup cases using real
shared physical backing. Each task stops exactly once, with no payload or
capability references left. See [M10 verification](docs/M10-VERIFICATION.md)
and [raw evidence](docs/evidence/m10) for exact commands and results.
All **25 final verification commands** passed, including panic, unexpected page
fault and timeout detection.

The M9 checkpoint passed **54 Rust tests**, **49 Python tests**, format and Clippy,
normal QEMU boots at **128 and 256 MiB**, and three external-module variants.
The same kernel ELF returned **42 or 99 depending on the external file**; a bad
version or a bad final opcode was rejected before admitting any program.
Panic, unexpected page fault and timeout paths also passed: **24 recorded commands**.
See [M9 verification](docs/M9-VERIFICATION.md) and [raw evidence](docs/evidence/m9).

The M8 checkpoint passed **49 Rust tests**, **42 Python validator tests**, format
and Clippy checks, normal QEMU boots at **128 and 256 MiB**, and the intentional
panic, unexpected page fault and timeout paths. All 18 verification commands passed.
Each normal boot validates 17 M8 dispatches, non-yielding loop isolation, shared
physical backing, revocation during suspension, admission quotas and cleanup.
See [M8 verification](docs/M8-VERIFICATION.md) and [raw evidence](docs/evidence/m8).
QEMU checks are local Windows/WSL results; portable checks also run in GitHub CI.

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
- Fixed-capacity tables, built-in native fixtures, and a bounded external bytecode
  loader. No general native application loader or dynamic loading.
- Compression and deduplication are opt-in experiments in a small policy domain.
- The interpreter and its host adapter are part of the trusted computing base.
- The initial read manifest is compiled into the kernel. Module order is trusted;
  there is no signature check or code-identity authentication.
- Revocation denies future object reads; it cannot erase values already read.
- Session cleanup covers normal scope exit and early returns. It is not guaranteed
  on `panic=abort`, power loss, or deliberate `mem::forget` in trusted kernel code.
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
| `docs/M9-FORMAT.md` | External package format, quotas and trust boundary |
| `examples/bytecode/demo.json` | Editable external bytecode example |
| `docs/M10-DESIGN.md` | Code/authority lifetime and automatic cleanup |
| `docs/M11-DESIGN.md` | Trusted initial grants and external shared reads |
| `examples/bytecode/shared.json` | External shared-object readers and denied task |
| `docs/M12-DESIGN.md` | Scoped revocation and restart boundaries |
| `examples/bytecode/revocation.json` | External read/yield/read lifecycle fixture |
| `docs/M1-VERIFICATION.md` … `docs/M12-VERIFICATION.md` | Checkpoint reports |
| `docs/evidence/` | Historical raw results, commands and source hashes |

Detailed engineering documents are currently written in Japanese. Historical
evidence includes the original machine's local paths; those paths describe the
recorded environment and are not prerequisites for building your checkout.
M8 adds bounded bytecode scheduling; see its [design and limits](docs/M8-DESIGN.md).
M9 loads externally packaged programs; see the [format and packer instructions](docs/M9-FORMAT.md).
M10 binds external execution to one authority host and cleans up on scope exit.
M11 adds a trusted read manifest and proves physical sharing by external programs.
M12 revokes through the bound host and tests resumed and restarted readers.
The historical v0.1.0 release remains the M7 checkpoint.

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
