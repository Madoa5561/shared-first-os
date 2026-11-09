# Milestone 8 検証報告

2026-11-09 JST。固定容量の制限付きバイトコードスケジューラを実装し、M8のゲートを通過しました。
M7の`run`ではYIELDなしプログラムが生涯燃料を使い切るまで実行を占有します。M8は命令数quantumで
中断してround-robinで交代するため、別の正常taskが先に完了できます。
外部ローダより先にこの実行資源の境界を固める判断と脅威モデルは [M8-DESIGN.md](M8-DESIGN.md) にあります。

## 実装内容

- `src/sandbox.rs`: `run_slice`、命令境界での中断と状態保存。既存runと8命令の意味は維持。
- `src/scheduler.rs`: 最大4slot、1〜64命令quantum、batch累積燃料2,097,152。
  終了/取消による予算払い戻し・slot再利用なし。不正admissionは状態不変。
  hostへ渡すtask IDをslotから固定し、HALT/fault/cancel時のcleanupを1回だけ呼ぶ。
- `src/m8.rs` / `src/m7.rs`: M7の実物理storeを再利用したQEMU試験を接続。
  不正task・正常taskの混在、強制中断中の失効、取消後の共有先寿命、最後の参照の回収。
- `src/lib.rs`: portable schedulerを公開。
- `tools/m8_validation.py` / `tools/test_m8.py` / `tools/qemu.py`:
  17dispatchの順序・結果・燃料を独立に照合し、偽の共有・残量補充・漏れ・欠落ログを拒否。
- README、ARCHITECTURE、ROADMAP、BUILD、DECISIONS、M8設計/報告と証跡を更新。

外部依存追加・ホスト設定変更・Git commit/push・release更新は実行していません。
公開済みv0.1.0はM7のままです。

## 実行と結果

```powershell
./tools/project.ps1 verify
```

[commands](evidence/m8/commands) に18コマンドの引数・標準出力/エラー・終了コードを保存しました。
ラッパーの18コマンドと全体は全て終了コード0です。QEMU自身の非0コードは次のとおり検査しています。

| 検査 | 実測結果 | 証跡 |
| --- | --- | --- |
| fmt / host・kernel全feature Clippy | 合格 | commands/02・04・05 |
| Rust単体 | 49 passed（M8追加6件） | commands/03 |
| Python検証器 | 42 passed（M8追加4件） | commands/06 |
| QEMU 128 MiB | M1〜M8 PASS、code 33、68.582秒 | [result](evidence/m8/boot-128M/result.json) |
| QEMU 256 MiB | M1〜M8 PASS、code 33、69.684秒 | [result](evidence/m8/boot-256M/result.json) |
| 2秒timeout | 期待どおり終了、code -9 | [result](evidence/m8/timeout/result.json) |
| 意図的panic | 期待どおり停止、code 35 | [result](evidence/m8/panic/result.json) |
| 想定外page fault | 診断して停止、code 35 | [result](evidence/m8/unexpected-fault/result.json) |

既存のM4実CPL3・独立CR3・4 MiB共有/COW、M5の18試行、M6の198試行、M7の22攻撃と36比較も
各正常起動で回帰検証しました。Rust 1.96.0、固定Limine、QEMU 10.2.1、OVMF、q35/qemu64/TCG/1CPUです。
ネットワーク・物理ホストディスクは接続していません。新しいGitHub CI・実機起動の検証はありません。

## M8の観測

1. quantum=2で暴走loop、正常42返却、YIELD連打、scratch境界違反を混在実行。
   正常taskはloopの燃料切れより先に終了し、残りは自分のfault/fuelだけで停止。
2. 別の2Contextが同じ4 KiB物理ページを読み、checksumは両方28,672。
   `M8SHARED`に実backingのアドレス一致とpayload=1/capabilities=3/mappings=0を記録。
   bytecode向けPTEは作らず、M7 BackendがHHDMのROを検証して読む。
3. 未grant taskは生きたobjectへのアクセスを拒否。2つの正当readerはSUM直後にquantum中断。
   その間にrevokeし、再開後のSUMはAuthorityで停止。trace件数0で物理読取りに到達しない。
4. task取消は残る共有先のpayloadを保持。最後のtaskのHALTでpayloadを回収。
   0/過大燃料、集計上限、5番目のslot、不正quantumを拒否。終了後も予算/slotの再使用を拒否。

| 正常bootの最終会計 | 128 MiB | 256 MiB |
| --- | --- | --- |
| allocator使用frame（前→後） | 89→89 | 153→153 |
| ページテーブルframe（前→後） | 73→73 | 137→137 |
| 仮想mapping数（前→後） | 31,390→31,390 | 64,158→64,158 |
| M8 payload / capabilities（最後） | 0 / 0 | 0 / 0 |

Scheduler全体608バイト（4Context込み）、試験host状態72バイトは既存64 KiB kernel stack内です。
最大の試験命令配列は共有/失効scenarioの176バイト。traceコピー等の一時変数も別にあり、これらの
size_ofを実stack最大使用量と混同しません。M7 store 24,384バイト・trace 520バイトを再利用し、
それらはカーネル画像内です。今回の画像範囲は864,256バイト（M7より24,576バイト増）。
構造体や既存stackを追加物理RAMとして二重加算しません。M8固有の性能比較・RAM削減の主張はありません。

## 生成物と完全性

正常版ELFをverify最終工程で復元し、実起動済みのISOを保持しています。

| ファイル | SHA256 |
| --- | --- |
| `build/normal/shared-first.iso` | `6d9398127c053eb7e86ff255491f6f239de9e0f6437f848c05ef8357f8c108ef` |
| `target/x86_64-unknown-none/release/shared-first-kernel` | `27df86c584315998d463b233aebd440ae3fa21d82dafdbf1b0897ac6ac924caa` |

[証跡コピー照合](evidence/m8/artifact-verification.json) は元ログと保存ログのhashを照合します。
[source-hashes.json](evidence/m8/source-hashes.json) にこの検証時点のソースを記録します。
以前のM1〜M7証跡は変更していません。ISOタイムスタンプ込みのbit再現性は保証しません。

## 残る制約と次段階

- 公平性は命令境界、最大3回の他task dispatch待ち。SUM最大256 KiBは同期処理で中断しない。
  信頼されたhost.stop等の処理時間も含め、実時間応答やnative preemptionを保証しない。
- 固定batch/組込みプログラム。slot再利用、外部ロード、汎用task生成、SMPは未実装。
- Schedulerは外部storeを所有しない。生きたbatchを捨てる前のshutdown、同一hostへの固定、
  fresh batch前の空task表を管理者契約とする。形式証明や任意nativeコードの隔離実装ではない。
- 次はversion付き外部bytecodeコンテナ、入力長/命令検証、コード保持量のquota、
  immutable Programの所有期間とtask終了の接続を実装する。ロード失敗時のrollbackと
  不正入力試験を先に整え、権限は信頼されたadmission側で付与する。
