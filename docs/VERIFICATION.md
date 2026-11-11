# Verification

Shared-First OS publishes controlled research results, not general performance
or production-security claims. The reproducible entry point is:

```powershell
./tools/project.ps1 verify
```

## Launch model corpus

The kernel executes 240 generated packages through the public bounded runner.
Sixteen seeds vary arithmetic, shared reads, non-yielding loops, scratch bounds,
initial fuel and READ manifests. Each seed is checked with quantum 1/2/64 and
dispatch limit 0/1/2/7/32.

A separate Rust model predicts dispatch count, limit cancellation, all terminal
states and remaining fuel without using the interpreter, scheduler or package
parser. Every runtime result is compared field by field. A Python queue-based
model independently predicts the aggregate digest and successful backend reads.

An additional 264 malformed inputs cover all 256 truncation positions and eight
header, entry or opcode mutations. They must fail during loading, before physical
allocation or a backend read. Every case checks resource balance; generated cases
also verify that reads within a case use the same physical backing.

This is a deterministic finite corpus. It is not exhaustive fuzzing, universal
model checking or a formal proof. The corpus exercises selected language operations
and is supplemented by the existing interpreter and object-store tests.

During development, an initial reference-model assumption incorrectly cleared
fuel after a rejected instruction. It was corrected to preserve unspent fuel,
matching the specified instruction behavior. A validator initially confused the
summary and completion record prefixes; the record names were separated. Neither
failure is counted as a passing verification run.
The initial validator failure is preserved separately in the
[initial host check](evidence/launch-model/initial-check/07.log).

## Results

The final run passed 76 Rust unit tests, three Rustdoc borrowing tests and 65 Python
validator tests, plus formatting and host/kernel Clippy. All 33 recorded verification
commands succeeded. [Exact commands and exit status](evidence/launch-model/commands)
are retained alongside the raw serial logs.

| Test | Outcome | Evidence |
| --- | --- | --- |
| Normal boot, 128 MiB | All suites passed, exit 33 | [result](evidence/launch-model/boot-128M/result.json) |
| Normal boot, 256 MiB | All suites passed, exit 33 | [result](evidence/launch-model/boot-256M/result.json) |
| External value change | Changed value observed; corpus passed | [result](evidence/launch-model/module-alt/result.json) |
| Invalid package version | Rejected before admission; corpus passed | [result](evidence/launch-model/module-bad-version/result.json) |
| Invalid package opcode | Rejected before admission; corpus passed | [result](evidence/launch-model/module-bad-opcode/result.json) |
| Shared read length change | Changed length observed; corpus passed | [result](evidence/launch-model/shared-alt/result.json) |
| Invalid shared opcode | Rejected before grants; corpus passed | [result](evidence/launch-model/shared-bad-opcode/result.json) |
| Shared manifest count mismatch | Rejected before grants; corpus passed | [result](evidence/launch-model/shared-bad-count/result.json) |
| Invalid re-read module opcode | Rejected before grants; corpus passed | [result](evidence/launch-model/revocation-bad-opcode/result.json) |
| Intentional two-second timeout | Process killed and reaped, exit -9 | [result](evidence/launch-model/timeout/result.json) |
| Intentional panic | Diagnostic and expected exit 35 | [result](evidence/launch-model/panic/result.json) |
| Unexpected page fault | Diagnostic and expected exit 35 | [result](evidence/launch-model/unexpected-fault/result.json) |

Failure-path success means the expected failure was detected, not a normal boot.

The final normal boots completed in 50.235 seconds at 128 MiB and 50.431 seconds
at 256 MiB, both with QEMU exit 33. The corpus digest is 1,494,677,361 and the
number of successful backend reads is 66 on both configurations.

| RAM | Frames before/after | Generated peak | Rejected-input peak | Page-table frames | Virtual mappings |
| --- | --- | --- | --- | --- | --- |
| 128 MiB | 89 / 89 | 90 | 89 | 73 | 31,410 |
| 256 MiB | 153 / 153 | 154 | 153 | 137 | 64,178 |

After every case, payload and capability counts return to zero. Rejected inputs
allocate no physical payload frames and perform no backend reads.

## Scope and accounting

The suite includes boot diagnostics, physical allocation, page permissions,
native CPL3 sharing and COW, optional memory policies, benchmark baselines,
interpreter attacks, external package changes/rejection, scoped cleanup,
revocation, snapshots and bounded launch behavior.

Native sharing checks identical physical backing for 1,024 pages across isolated
tasks; a single COW write allocates one private 4 KiB page. Ordinary sharing/COW
and object sharing/COW have equal payload requirements in the compared workloads.
Guest TSC ticks are not calibrated seconds or physical CPU cycles. Restricted
bulk host reads and native byte loops are different execution mechanisms.

Physical frames, metadata, mappings and retained boot regions are accounted
separately. No general memory or speed improvement is claimed from the additional
validation code. Stack high-water usage and optimized secure erasure are unmeasured.

The kernel image reserves 946,176 bytes and the three original modules retain
12,288 bytes, for 958,464 bytes in the kernel/module regions. The kernel region
grew by 8,192 bytes relative to the previous bounded-launch build. The fixed
Report, Completion, Batch and Session values occupy 272, 176, 1,096 and 648 bytes;
these stack values are not additional payload frames.

Verified image SHA-256:

- ISO: `50a267f025b1145042e4743f463ebcd359bd0f8c356b0dc814354af31493cfbb`
- ELF: `0b3898d53a39f5ace6d994f07670675a344a377092516ceaac320eb4156acc06`

The [artifact manifest](evidence/launch-model/artifact-verification.json) records
the binary and copied-log hashes. The [source manifest](evidence/launch-model/source-hashes.json)
maps public source, configuration and documentation to this verification.

## Reproduction and limitations

The verified platform uses Windows Rust 1.96.0 and WSL2 Ubuntu 26.04 with QEMU
10.2.1, OVMF, q35, TCG, one CPU and 128/256 MiB RAM. See [setup](BUILD.md).
CI checks portable Rust/Python code; QEMU evidence comes from local Windows/WSL.
ISO timestamps are not fixed, so bit-identical ISO rebuilds are not claimed.

The interpreter, kernel manifest and module order are trusted. Hardware, SMP,
native preemption, DMA isolation, side channels, general native applications and
formal proofs are outside the verified scope. Host allocation/seal failure
injection is unit-test evidence, not a hardware failure experiment. The defensive
Session-construction error branch is not fault-injected because validated input
and prechecked configuration cannot currently reach it.

Raw serial identifiers and older evidence paths are retained for reproducibility.
They identify test suites and historical measurements.
