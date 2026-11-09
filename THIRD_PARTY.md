# Third-party components

Original project code is licensed under the root `LICENSE` (MIT). The upstream
terms below remain applicable to the identified third-party material.

The kernel's Limine request constants and C-compatible structure layouts in
`src/boot.rs` and `src/memory.rs` follow Limine v8.7.0 `limine.h`.
The corresponding header license is reproduced below:

```text
BSD Zero Clause License
Copyright (C) 2022-2025 mintsuki and contributors.

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY
SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION
OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
```

Limine boot binaries are downloaded separately into `.tools/limine/`, along with
their upstream `LICENSE` (also preserved in `LICENSES/Limine.txt`). Version,
immutable commit and hashes are recorded in
`tools/versions.json`. The generated ISO includes these bootloader binaries.
QEMU, OVMF, xorriso and binutils are Ubuntu packages, not kernel source dependencies.
