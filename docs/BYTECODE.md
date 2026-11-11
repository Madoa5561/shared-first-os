# Bytecode packages

External programs use a bounded custom format, not native ELF or WebAssembly.
The complete package is validated and copied into an owned Batch before any
initial capability grant. Programs borrow that immutable copy during execution.

## Container version 1

All integers are little-endian. A 16-byte header is followed by the declared
number of entries, each consisting of a 16-byte entry header and its code.
Trailing bytes are rejected.

| Header offset | Bytes | Value |
| --- | --- | --- |
| 0 | 8 | ASCII `SFOSBC09` |
| 8 | 2 | Version 1 |
| 10 | 2 | Program count, 1..4 |
| 12 | 4 | Zero |

| Entry offset | Bytes | Value |
| --- | --- | --- |
| 0 | 4 | Code size, 16..256, multiple of 16 |
| 4 | 4 | Positive lifetime fuel |
| 8 | 8 | Zero |
| 16 | Code size | Instructions |

Total code is at most 512 bytes, the package at most 592 bytes and aggregate fuel
at most 2,097,152. Fixed code storage reserves four 256-byte slots. The format
carries no rights, task IDs, native entry point or physical addresses.

## Instructions

Each instruction is 16 bytes: opcode, dst, src, aux (one byte each), an unsigned
64-bit immediate and four zero reserved bytes. Registers are numbered 0..3.
Each task starts with zeroed registers and 64-byte scratch, except that trusted
launch policy may supply a task-local handle in r1.

| Opcode | Operation | Meaning |
| --- | --- | --- |
| 0 | HALT | Return r0 and stop |
| 1 | CONST | dst = immediate |
| 2 | SUM | dst = checked object sum(handle=src, offset=aux, length=immediate) |
| 3 | LOCAL_LOAD | dst = scratch[register src] |
| 4 | LOCAL_STORE | scratch[register src] = low byte of dst |
| 5 | ADD | dst = dst + src, wrapping unsigned arithmetic |
| 6 | JUMP | Jump to immediate instruction index |
| 7 | YIELD | Return r0 while preserving the context |

HALT/YIELD require zero operand fields. CONST requires zero src/aux.
LOCAL_LOAD/LOCAL_STORE/ADD require zero aux/immediate. JUMP requires zero register
fields and an in-range target. SUM is limited to 256 KiB. All reserved bytes must
be zero, and all code is checked even when unreachable.

Every instruction costs one fuel unit; SUM costs one plus its byte length.
Insufficient fuel prevents the instruction and preserves the unspent amount.
Yield does not refill fuel. Bounds or authority faults stop the task.

## Packaging and launching

```sh
python3 tools/pack_bytecode.py examples/bytecode/demo.json build/programs.sfbc
```

JSON contains a `programs` array with `fuel` and `instructions` per program.
Each instruction is `[opcode,dst,src,aux,immediate]`. The packer checks integer,
size and quota limits; the kernel performs final instruction validation. Input
JSON is limited to 64 KiB and the packer refuses to overwrite an existing output.

The image builder packs three examples automatically. Boot module order is trusted
configuration. The read manifest uses exactly three programs and one non-executing
owner slot; only selected readers receive capabilities. Valid package structure
alone does not grant rights or authenticate program identity.

`runner::execute` accepts package bytes, an empty object store and trusted options.
It returns an owned result after teardown. Its quantum is 1..64 and dispatch limit
0..1024. A zero dispatch limit prepares then cancels without executing. A busy
store is rejected without reclaiming existing resources.

The automated QEMU validator expects particular fixtures. Repacking an example
does not modify an existing ISO; use the image builder and a manual QEMU run to
inspect a custom workload. This is not a general application shell.
