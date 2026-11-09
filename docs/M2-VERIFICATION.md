# Milestone 2 検証報告

この文書と `evidence/m2/` はM2時点の履歴です。通常の生成物パスはM7のビルドで更新されました。
現在のISO・ELFのハッシュと検証結果は [M7検証報告](M7-VERIFICATION.md) を参照してください。

2026-11-08 JST。M2の受け入れ条件を、Rustホストテストと実QEMU起動で検証しました。
独自の物理フレーム管理とページテーブルを使い、割当て・写像・保護違反・回収まで動作しています。

## 生成物と再実行

リポジトリのPowerShellで実行した一括コマンドは次のとおりです。

```powershell
./tools/project.ps1 verify
```

一括検証の終了コードは0、内部の全18コマンドも0です。
QEMU自身の終了コードは、正常33、想定したパニック／異常ページフォルト35、
意図的なタイムアウト試験ではkill・wait後の-9です。

| 生成物 | パス | SHA256 |
| --- | --- | --- |
| 正常版UEFI ISO | `build/normal/shared-first.iso` | `935a47aa62164ff3e9b4f3db395e68fd03213e7f58c4e6ba4ab6c2a9a5e3c1ed` |
| 正常版Rust ELF | `target/x86_64-unknown-none/release/shared-first-kernel` | `72703aff4fcfa94bb6594a6827c94d96f82bc32529365bec88db5e53408a3737` |

最終ISOが128／256 MiBの実起動で使ったISOと同一ハッシュであること、
最終ELFがそのISO作成時のELFと同一ハッシュであることを確認しています。
固定したツール版はM1と同じで、新しい依存関係は追加していません。

## 検証結果

| 検証 | 結果 | 証跡 |
| --- | --- | --- |
| Rust整形 | 成功 | [fmt](evidence/m2/commands/02.log) |
| ホスト単体テスト | 14件成功 | [テスト出力](evidence/m2/commands/03.log) |
| Clippy、警告をエラー化 | ホスト・カーネルとも成功 | [ホスト](evidence/m2/commands/04.log)、[カーネル](evidence/m2/commands/05.log) |
| Python検証器 | 11件成功 | [出力](evidence/m2/commands/06.log) |
| Rust releaseビルド | 成功 | [出力](evidence/m2/commands/07.log) |
| UEFI ISO作成 | 成功 | [xorrisoの実行記録](evidence/m2/image-normal/command.json) |
| QEMU 128 MiB | `M1 PASS`、`M2 PASS`、終了33 | [シリアル](evidence/m2/boot-128M/serial-readable.log)、[結果](evidence/m2/boot-128M/result.json) |
| QEMU 256 MiB | `M1 PASS`、`M2 PASS`、終了33 | [シリアル](evidence/m2/boot-256M/serial-readable.log)、[結果](evidence/m2/boot-256M/result.json) |
| 早期Rustパニック | 意図した診断、終了35 | [結果](evidence/m2/panic/result.json) |
| 想定外ページフォルト | CR2/error/RIPを診断、終了35、M2 PASSなし | [シリアル](evidence/m2/unexpected-fault/serial-readable.log)、[結果](evidence/m2/unexpected-fault/result.json) |
| タイムアウト | 停止した実QEMUを2秒後にkill・wait | [結果](evidence/m2/timeout/result.json) |
| ELF確認 | x86-64 EXEC、RX/R/RWセグメント | [readelf](evidence/m2/elf.log) |

正確な引数配列・作業ディレクトリ・各終了コードは [commands](evidence/m2/commands/) にあります。
生シリアルログも各bootフォルダの `serial.log` に保存しています。
合成ログを使うPythonテストは検証器の単体テストであり、OS起動の証明ではありません。

## 実QEMUで観測したフレーム会計

単位は4 KiBフレームです。下表のtotalはusableプールで、物理アドレス0を除きます。

| QEMU RAM | total | 試験終了時allocated | free／枯渇試験で確保して回収した数 | ページテーブル | 新スタック |
| --- | ---: | ---: | ---: | ---: | ---: |
| 128 MiB | 30,510 | 89 | 30,421 | 73 | 16 |
| 256 MiB | 63,278 | 153 | 63,125 | 137 | 16 |

両方で `total = allocated + free`、`allocated = table_frames + stack_frames` を確認しました。
payloadフレームと、一時写像のために増えたページテーブルは全て回収しました。
永続テーブルとスタックは動作中なので解放していません。

全割当て先を元のメモリマップと独立に照合し、usableの内側に完全に入ることと、
予約領域に接触しないことを確認しました。枯渇試験では別のビットマップで重複を検出します。
仮想ページ写像数は31,240／64,008で、これを物理メモリ使用量として加算していません。
allocatorメタデータ65,536バイト、試験用メタデータ32,768バイト、メモリマップ保存配列
98,304バイトはカーネル画像の予約領域内にあります。旧ブートローダ用メモリは保持しています。

128 MiBのシリアル出力末尾は次のとおりです。

```text
M2FRAMES total=30510 allocated=89 free=30421 table_frames=73 stack_frames=16 payload_frames=0 bitmap_bytes=65536 scratch_bytes=32768 mappings=31240
M2CHECK exhausted_reclaimed=30421 reserved_excluded=1 map_unmap=1 tlb=1 readonly=1 nx=1 guards=2 wx_aliases=1 faults=7
M2 PASS
```

## ハードウェアで確認した保護

| アクセス | CPUページフォルトのエラー値 |
| --- | ---: |
| 下側ガードページの読取り | 0（未写像） |
| 上側ガードページの読取り | 0（未写像） |
| カーネルコードVAへの書込み | 3（present、write、supervisor） |
| 同じコードのHHDM別名への書込み | 3 |
| unmap・INVLPG後の元VAの読取り | 0 |
| RO payload写像への書込み | 3 |
| NX payloadからの命令取得 | 17（present、instruction fetch） |

全7件でCR2・エラービット・例外RIPを照合して復帰しました。
新スタック上に実際のRSPがあることも確認しました。
unmap後に同じVAを別の物理フレームへ写像し、別のデータが読めることで古いTLB参照の残存を検査しました。
kernelの全ページについて通常VAとHHDM別名の物理対応と権限も検査しています。

想定外フォルト専用ISOでは、同じ試験VAを復帰条件を設定せずに読み、以下の診断で停止しました。

```text
unexpected page fault: cr2=0xffffff0000000000 error=0x0 rip=0xffffffff80001000
```

これはユーザータスク間の隔離やCapability失効の試験ではありません。
すべてring 0、1 CPUのカーネル内試験です。

## ホスト単体テストの範囲

- usableの内側への整列、物理0除外、重複するusable記述子の二重計上防止。
- 予約・未知・ACPI・ブートローダ・カーネル領域による割当て候補の除外。
- 枯渇、再利用、未割当て／二重／不正解放、容量超過、入力オーバーフロー。
- 10,000操作の決定的な割当て・解放と独立した所有モデルの一致。
- map済みVAの上書き拒否、非canonical／非整列アドレス拒否。
- ページテーブル確保途中の失敗のロールバック、隣接写像の保持、空テーブル回収。
- 解放に先行するTLB無効化イベントの確認。実 `INVLPG` はQEMU側で検証。
- M1から継続したメモリ集計の異常値・整数オーバーフローテスト。

## 修正した問題

途中のClippyでは、異常系試験の機能フラグ・補助関数の未追加を検出しました。
差分適用に失敗していた追加箇所を補いました。
続いてunsafe直前の安全条件コメント不足と、raw pointerからの暗黙的な参照生成を修正しました。
各失敗の終了コード101と出力は [failures](evidence/m2/failures/) に残しています。
修正後のClippyと一括検証は全て通っています。M2の実QEMU起動失敗は発生していません。

## 変更ファイル

| 分類 | ファイル |
| --- | --- |
| 新規の中核 | `src/frame.rs`、`src/paging.rs`、`src/m2.rs`、`src/faults.rs` |
| 起動・ハードウェア接続 | `src/boot.rs`、`src/machine.rs`、`src/main.rs`、`src/lib.rs`、`linker.ld` |
| 試験と機能フラグ | `Cargo.toml`、`tools/project.ps1`、`tools/qemu.py`、`tools/test_qemu.py` |
| 文書 | `README.md`、`docs/ARCHITECTURE.md`、`docs/ROADMAP.md`、`docs/BUILD.md`、`docs/DECISIONS.md`、`docs/M1-VERIFICATION.md`、この報告書 |
| 保存証跡 | `docs/evidence/m2/` |

M1の保存証跡は残しています。依存追加、Git初期化、commit/push、物理ディスクへの書込みは行っていません。

## 制限とM3へ残る作業

usableの物理終端は1 GiB以下、4段ページング・4 KiBページ・1 CPUのみです。
QEMUの128／256 MiBで検証しました。実機、SMP、BIOS、PCID、LA57、huge pageは未検証または未対応です。
ガードページの保護は実証しましたが、実際のスタック枯渇からの安全な復帰は未実装です。
IDTはページフォルト用だけで、TSS/IST、double-fault専用スタック、他の例外処理はまだありません。

生のフレームAPIは信頼されたカーネル内部専用です。M3ではオブジェクトによるフレーム所有と、
タスク固有の世代付きCapability、権限縮小、参照数、部分失敗時の回収、写像の寿命と
失効時のPTE除去・TLB無効化を実装します。ring-3での隔離・同一物理ページ共有・COWの実証はM4です。
