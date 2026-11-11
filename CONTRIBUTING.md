# Contributing

Shared-First OS is a research prototype. Please open an issue describing the
problem or proposed experiment before a substantial feature change.

## Development

1. Read [the architecture](docs/ARCHITECTURE.md), [security model](docs/SECURITY-MODEL.md),
   and [build instructions](docs/BUILD.md).
2. Keep changes focused. Document safety invariants at every necessary unsafe
   hardware/memory boundary.
3. Run `./tools/project.ps1 check` for format, Clippy and host tests.
4. Run `./tools/project.ps1 verify` for kernel/runtime changes and preserve the
   generated command, serial and result logs.
5. Explain the change, tested environment, results, and unverified scope in your PR.

A `cargo check` result is not proof that the kernel boots. Synthetic validator
tests are not substitutes for actual QEMU evidence. Compare against ordinary
sharing/COW, account for metadata, and avoid unsupported performance claims.

Do not submit credentials, personal data, generated build trees, or firmware
variable stores. Review diagnostic logs before publishing them. Never use a
physical disk as an early testing target.

Contributions are provided under this project's MIT license, except for files
with an explicitly preserved upstream license. Keep all third-party notices.
