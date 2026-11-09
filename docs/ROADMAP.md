# Roadmap and gates

## 現在の状態（M10）

M1〜M10のゲートを通過しました。M10の実行セッションと自動cleanupを実装・検証しました。
M7は脅威モデルに限定した言語隔離の研究試作です。
実ring-3の共有・隔離・COW、任意の重複排除と圧力時圧縮、5方式のベースラインに加え、
制限付きバイトコードの検証・燃料・Capability読取り・失効を実装しました。
検証コマンドは `./tools/project.ps1 verify`、証跡と正確な結果は
[M10-VERIFICATION.md](M10-VERIFICATION.md) に記録します。
M10も固定容量の有限試作です。汎用OSや安全性証明の完成ではありません。

M4ではGDT/TSS・ring-3移行・int 0x80・独立CR3を実装しました。呼び出し元はカーネルが決定し、
ユーザー引数で他タスクのCapability表を選べません。4 MiBの全1,024ページの物理アドレス一致、
1ページだけのCOW、元の内容の保持、失効後のユーザーfault、全資源回収を実証しています。
現在は単一CPU・協調実行・組込みユーザーコードによる有限試験です。

M5は専用storeでimmutableページを内容照合して集約し、未写像ページだけを圧縮・再map時に復元します。
3負荷×有効／無効×3回を各RAM構成で実行しました。実CPL3の読取り・COW・独立失効・回収も維持しています。
圧縮用予約領域を含む管理コストを計上し、非圧縮性データの追加コストとゲストTSC tickを記録しています。
圧力は制御したしきい値で発生させた試験であり、自然なOOMや汎用的な高速化の実証ではありません。

M6は16／256 KiB、dirty 0／1／全ページの22条件で、各1回のウォームアップと8回の測定を実行します。
allocatorのピーク、画像内の管理メモリ、fault／コピー回数、診断を除いたゲストtickとthroughputを保存します。
通常共有／COWとobject方式のpayload量は等しく、Capability方式だけの新しい節約とは解釈しません。
既存OSのmmap実装、実CPU時間、多数タスク、M5 adapter自体の素のBackendに対する全コストは未比較です。

M7は [脅威モデル](M7-THREAT-MODEL.md) を実装前に定義し、言語隔離・paging・Capabilityハードウェアを比較しました。
制限付きインタプリタを試作し、22種類の攻撃検査と、16／256 KiBのnative／bytecode比較を各8回測定しました。
nativeのpaging保護を維持し、bytecodeだけが信頼されたインタプリタを介してkernel CR3を共有します。
同一バイナリのM6相当対照群に対する19フレーム削減を測定しました。未使用秘密ページを含む既存対照群であり、
最小native実装・汎用アプリの性能や安全性に一般化しません。形式証明・WASM・CHERI実装はありません。

## M1: Bootable kernel (do this first)
- Confirm host prerequisites and pick compatible pinned Rust / Limine / QEMU versions.
- Rust `no_std` + explicit target/linker config + Limine boot image.
- Boot under QEMU; log `Shared-First OS booted` over COM1 serial.
- Parse and display firmware memory map, usable/reserved region totals; never treat reserved regions as allocator inputs.
- `make run` (or documented equivalent) and `make test-boot` with timeout, serial marker assertion and deterministic exit mechanism.
- Include exact Windows+WSL2/Ubuntu setup instructions if on Windows.
- Gate: real QEMU serial transcript and exit code; if not run, mark UNVERIFIED.

## M2: Physical memory & paging
- Bitmap/buddy allocator of usable physical frames, correct reserved/bootloader treatment.
- Page-table mapping/unmapping, W^X where practical, kernel stack guard where feasible, TLB handling.
- Host unit tests for allocator and failure cases, QEMU allocation/reclaim checks.
- Gate: no double allocation; no reserved frame use; deterministic tests.

## M3: Capability object store
- Object descriptor, frame backing, reference accounting, permission system and per-task handles with generations.
- Immutable shared payload, authority-checked map/release/revoke APIs.
- Unit tests covering stale handles, rights downgrade, double free, overflow and partial failure cleanup.
- Gate: model-backed tests and kernel integration of object store.

## M4: Ring-3 tasks and true sharing
- Minimal task switch / user transition, syscall boundary, per-task address spaces.
- Shared physical pages mapped read-only to two real isolated tasks.
- COW page-fault handling and writable mappings controlled by rights.
- Gate: QEMU demo proving physical-frame identity, isolation, COW and clean reclamation. No fake user-task simulator as substitute.

## M5: Adaptive policies
- Optional read-mostly object interning/deduplication, pressure-triggered compression; correct metadata accounting.
- Explicit overhead and latency analysis; avoid compressing already compressed/random data when harmful.
- Gate: enabled/disabled A/B tests on identical workload.

## M6: Baselines and benchmark report
- Implement fair baseline modes for anonymous/private/shared/COW where relevant.
- Report peak physical frames, page faults, COW copies, metadata, throughput and latency.
- Gate: reproducible script, raw results, sample sizes, correct interpretation of where results generalize.

## M7: New isolation paradigm (research stage)
- Compare language-isolated single address space, common mappings with paging, and capability-hardware-inspired approaches.
- Prototype only with explicit threat model and proof/test plan; cannot assert arbitrary native-code isolation from handle checking alone.
- Gate: attack-oriented tests and measured memory savings relative to hybrid M4/M6 architecture.

## Quality gates across milestones
- Formatting, lint where available and host tests for portable components.
- No `unsafe` without stated invariants.
- No successful test claims without stdout/stderr and exit status.
- Maintain design decision log and run instructions.

## M8: Bounded bytecode scheduling

- 外部入力や命令を増やす前に、YIELDなしloopが他タスクの進行を妨げない実行基盤を作る。
- 最大4タスク、1〜64命令quantumのround-robin、batch累積燃料2,097,152。
- 生成時だけ燃料を付与し、yield・終了・取消で払い戻さない。slotのbatch内再利用は禁止。
- kernelがtask IDを固定。HALT/fault/cancel時に当該taskのCapability/写像を解放する。
- 強制中断中のrevoke後は次の読取りを拒否。共有の実物理backing・checksumを検査する。
- Gate: Rust/検証器テスト、128/256 MiB QEMUで実行順序と燃料残量、容量・権限拒否、
  共有・回収を照合し、M1〜M7の回帰と意図的な失敗経路も通す。
- 設計境界: [M8-DESIGN.md](M8-DESIGN.md)。外部ローダ、native preemption、実時間保証は含まない。

## M9: External bytecode packages

- Limine moduleで独立ファイルを取得し、version付きコンテナを検証して所有copyする。
- 1..4 program、各code 16..256 byte、code合計512 byte、ファイル最大592 byte。
- 全件検証後だけadmit。初期権限は空、file内のhandle値は権限付与にならない。
- 所有codeのborrowが全taskの終了まで有効で、終了後clearできる寿命規則。
- Gate: ホストの切断/余剰/型/予約bit/quota/opcode検査、同一ELFで別moduleの出力変更、
  不正version/最後の不正opcodeの全件拒否、実QEMUの回収とM1〜M8回帰。
- 形式・脅威モデル: [M9-FORMAT.md](M9-FORMAT.md)。元のLimine予約ページは保持。

## M10: Scoped execution lifecycle

- Sessionがコードのimmutable borrowと同じauthority hostのexclusive borrowを保持する。
- dispatchごとのhost付替えを禁止し、途中return/Dropで残るtaskだけを自動shutdown。
- M9外部実行をSessionへ移行。空Batch/不正設定はhost引受け前に拒否。
- Gate: 借用境界のcompile-fail/pass、通常終了/fault/取消/Dropの単体検査、
  QEMUで4ケースの実payload回収と各stop1回、frame/table/mapping収支、M1〜M9回帰。
- [M10-DESIGN.md](M10-DESIGN.md)。panic=abort/mem::forget/SMP等は保証対象外。

## M10後の未着手候補

- 次の優先候補: 信頼されたlaunch manifestによる初期Capability付与と、外部プログラムからの
  immutable object共有デモ。プログラムがtask IDや権利を自己申告して増幅できない設計にする。
- native preemptionと動的task slot再利用には別の設計・検証が必要。
- 検証器の体系的なfuzz・モデル検査、より強い証明、信頼基盤の縮小とruntime自身の隔離。
- 未使用秘密ページ除外・RXコード共有などを行ったnative対照群、複数task数・入力・書込み負荷での比較。
- 制限言語の機能拡張に先立つ脅威モデル更新。データ書込み・host機能の追加ごとに権限・寿命・会計を検証。

これらは次の研究課題で、今回実装済みという意味ではありません。
