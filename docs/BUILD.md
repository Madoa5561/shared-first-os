# Build and test

The verified environment is Windows PowerShell 7, Rust 1.96.0 with the MSVC host
toolchain, and WSL2 Ubuntu 26.04. QEMU uses TCG, one CPU and OVMF; nested hardware
virtualization is not required. The kernel has no external Rust crate dependencies.

## Prerequisites

Windows needs Rust/MSVC and the Visual Studio C++ build tools. WSL needs Python 3
and the packages below. Installing host tools is a separate setup operation;
normal project scripts do not install or update them.

```powershell
rustc --version
cargo --version
wsl --list --verbose
rustup target add x86_64-unknown-none
```

To add the pinned toolchain without changing your existing default:

```powershell
rustup toolchain install 1.96.0 --profile minimal --component rustfmt,clippy --target x86_64-unknown-none
```

Use `rustup run 1.96.0 pwsh -File ./tools/project.ps1 verify` when your default
toolchain differs. The script refuses an unexpected compiler version.

The recorded Ubuntu package versions are:

```sh
sudo apt-get update
sudo apt-get install --no-install-recommends \
  qemu-system-x86=1:10.2.1+ds-1ubuntu3 \
  ovmf=2025.11-3ubuntu7 \
  xorriso=1:1.5.6-1.1ubuntu4 \
  binutils=2.46-3ubuntu2
```

These exact packages may not exist in another Ubuntu release or repository
snapshot. The project pins direct dependencies in `tools/versions.json`;
it does not bundle a complete distribution snapshot.

## Fetch the bootloader

From the repository root in PowerShell:

```powershell
./tools/fetch-limine.ps1
```

This explicitly downloads three pinned Limine files into `.tools/limine` and
verifies their SHA-256 hashes. Existing mismatched files are not overwritten.
Normal builds run without network access.

## Commands

```powershell
./tools/project.ps1 check
./tools/project.ps1 build
./tools/project.ps1 test-boot
./tools/project.ps1 verify
```

- `check`: formatting, Rust unit tests, Rustdoc borrowing checks, host/kernel
  Clippy and Python validator tests.
- `build`: release ELF and bootable UEFI ISO, including three external modules.
- `test-boot` / `run`: the same finite headless workload at 128 MiB.
- `verify`: all checks, 128/256 MiB boots, seven altered-input boots, timeout,
  intentional panic and unexpected page fault. It restores the normal ELF last.

The full verification command currently records 33 subprocesses. A normal boot
has a 180-second deadline; the intentional timeout test uses two seconds.
QEMU success is exit 33, intentional panic/fault is 35, and timeout cleanup records
the killed process status. The wrapper translates expected outcomes into a
successful test command.

Each boot uses a fresh evidence directory and private OVMF variable-store copy.
QEMU has no network interface or host physical disk attached.

## Artifacts and evidence

| Path | Contents |
| --- | --- |
| `build/normal/shared-first.iso` | Bootable image |
| `target/x86_64-unknown-none/release/shared-first-kernel` | Kernel ELF |
| `artifacts/commands-*/` | Exact commands, output and exit status |
| `artifacts/boot-*/` | Serial log, results and benchmark tables |
| `artifacts/image-*/` | Image command, inputs and hashes |

Validators check complete records, frame identity, permissions, terminal states,
fuel and resource balance. A success marker alone is insufficient.
The launch-model corpus runs in addition to the native, object, COW, policy,
interpreter, external-input and lifecycle regressions.

Serial prefixes and archived directory names are stable test identifiers.
Historical evidence is retained with its original hashes.

## External packages

The builder packs the JSON examples into `programs.sfbc`, `shared.sfbc` and
`revocation.sfbc`. Package contents are validated before execution.
See [the format and packer](BYTECODE.md). The automated boot tests expect their
specific fixtures; arbitrary edited programs need a manual QEMU run for inspection.

After building the ELF on Windows, individual image and boot operations can
also be invoked from WSL in the same repository:

```sh
python3 tools/qemu.py image
python3 tools/qemu.py test --memory 128
python3 tools/qemu.py test --memory 256
```

## Troubleshooting and limits

Inspect `stderr.log`, `serial.log` and `result.json` for boot failures, and
`artifacts/commands-*/` for format, compiler or validator failures.
A timeout is a failure unless the test explicitly expects one.

ISO timestamps are not fixed, so bit-identical rebuilds are not claimed.
Physical-machine installation, BIOS boot, Secure Boot, KVM and GUI behavior are
outside the verified scope. GitHub CI runs portable checks; full QEMU evidence
comes from the recorded Windows/WSL environment.
