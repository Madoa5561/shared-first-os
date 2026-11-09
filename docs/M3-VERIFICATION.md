# Milestone 3 検証報告

この文書と `evidence/m3/` はM3時点の履歴です。通常の生成物パスはM7のビルドで更新されました。
現在のISO・ELFのハッシュと検証結果は [M7検証報告](M7-VERIFICATION.md) を参照してください。

2026-11-08 JST。M3のCapability object storeを実装し、ホストモデルと実QEMUで検証しました。
M3のゲートを通過しています。ring-3タスクの隔離・共有・COWはM4の未実装項目です。

## 生成物と再実行

リポジトリのPowerShellで実行した正式コマンド:

```powershell
./tools/project.ps1 verify
```

終了コード0。内部の全18コマンドも終了コード0です。
QEMU自身のコードは正常33、意図的パニック／想定外fault35、タイムアウト試験-9です。
最後に正常版ELFへ戻して、実際に起動した正常版ISOを保持しています。

| 生成物 | パス | SHA256 |
| --- | --- | --- |
| 正常版UEFI ISO | `build/normal/shared-first.iso` | `7e0d660697450708305054010412c95d3f8c665a53ae2f1371ae5af5e3f16b72` |
| 正常版Rust ELF | `target/x86_64-unknown-none/release/shared-first-kernel` | `359deb61ec7ad720cb4db0757847801d6591978dedac123d4b4a4261be89bd19` |

[生成物照合記録](evidence/m3/artifact-verification.json)で、最終ISOが両RAM構成の実起動ISOと、
最終ELFがISO作成時のELFと一致することを確認しました。
ツール・依存関係はM2と同じです。追加インストール、commit、push、branch作成は行っていません。

## 実施した検証

| 検証 | 結果 | 証跡 |
| --- | --- | --- |
| Rust整形 | 成功 | [fmt](evidence/m3/commands/02.log) |
| Rustホスト単体テスト | 25件成功 | [出力](evidence/m3/commands/03.log) |
| Clippy、警告をエラー化 | ホスト・カーネルとも成功 | [ホスト](evidence/m3/commands/04.log)、[カーネル](evidence/m3/commands/05.log) |
| Python検証器の単体テスト | 15件成功 | [出力](evidence/m3/commands/06.log) |
| Rust releaseビルド | 成功 | [出力](evidence/m3/commands/07.log) |
| QEMU 128 MiB | M1/M2/M3 PASS、コード33 | [シリアル](evidence/m3/boot-128M/serial-readable.log)、[結果](evidence/m3/boot-128M/result.json) |
| QEMU 256 MiB | M1/M2/M3 PASS、コード33 | [シリアル](evidence/m3/boot-256M/serial-readable.log)、[結果](evidence/m3/boot-256M/result.json) |
| 意図的なpanic | 想定どおりコード35 | [結果](evidence/m3/panic/result.json) |
| 想定外ページフォルト | 正確な診断後コード35 | [結果](evidence/m3/unexpected-fault/result.json) |
| 実QEMUのタイムアウト | 2秒後kill・wait、コード-9 | [結果](evidence/m3/timeout/result.json) |

全コマンドの引数・cwd・終了コードは `evidence/m3/commands/*.json`、stdout/stderrは対応する
`.log` に保存しています。QEMUの正確な引数とISO/ファームウェアのハッシュは各 `result.json` にあります。
Pythonの合成ログ試験は検証器のテストであり、実起動の代わりにはしていません。
このM3の正式検証ではコンパイル失敗・予期しない起動失敗はありませんでした。

## 共有と回収の実測

4,097バイトの入力を2ページに格納し、残り4,095バイトをゼロ化しました。
2つの論理タスク表のCapabilityを使い、同じ物理フレームを別々のVAにR/NXで写像しました。
両VAとHHDMのPTEを照合し、2つのVAから8,192バイト全てを実際に読んで一致を検査しました。

| 会計 | 128 MiB | 256 MiB |
| --- | ---: | ---: |
| usableプールの総フレーム | 30,441 | 63,209 |
| 試験前の割当済みフレーム | 89 | 153 |
| 共有試験中の割当済みフレーム | 95 | 159 |
| payload物理フレーム | 2 | 2 |
| 共有試験中のページテーブル | 77 | 141 |
| ページテーブルの増分 | 4 | 4 |
| オブジェクトの仮想写像ページ | 4 | 4 |
| Capability数 | 2 | 2 |
| 管理配列バイト数 | 4,928 | 4,928 |
| 全試験後の割当済みフレーム | 89 | 153 |
| 全試験後のページテーブル | 73 | 137 |
| 全試験後の全仮想写像ページ | 31,247 | 64,015 |

128 MiBでpayloadの物理アドレスは `0x5a000` / `0x5b000`、256 MiBでは `0x9a000` / `0x9b000`。
2番目の写像の追加時、割当フレーム増分がページテーブル増分だけであることも検査しました。
管理配列は既存64 KiBスタック内にあり、追加の物理フレームとして二重加算しません。
この値はbackend・入力・一時変数を含むカーネル全体のメタデータ総量ではありません。
file-backedとcompressed bytesは未実装で0です。仮想写像数はRAM消費量ではありません。

M3のCPU例外9回の内訳:

- HHDM別名2ページへの書込み拒否（エラー3、2回）。
- 2つの共有写像への書込み拒否（エラー3、2回）。
- NX写像からの命令実行拒否（エラー17、1回）。
- object-wide revoke後の両VAの読取り拒否（エラー0、2回）。
- 通常release後の各VAの読取り拒否（エラー0、2回）。

通常releaseでは、先に作成元を解放した後も共有先でデータを読めることを確認しました。
最後の共有先のreleaseでpayloadを回収します。失効後の古いハンドルは拒否され、再利用slotでは
世代が変わります。同じVAへの新オブジェクト写像は新しいデータを返します。

## 異常値・部分失敗の検証範囲

ホスト側では10件のobjectテストを追加しました。全割当て／seal／map段階での失敗、
既存写像との衝突、容量・サイズ・アドレス境界、権限の縮小・増幅拒否、異なるtask表、
世代上限での退役、参照数オーバーフロー、二重解放、1,000回の寿命サイクルとDropを検査します。
さらにページ権限変更のTLB無効化・物理アドレス保持テストを1件追加しました。

QEMUでは2ページ目のフレーム確保を決定的に失敗させ、確保済み1ページの回収を確認しました。
これは実backendへの失敗注入であり、自然なOOMの再現という意味ではありません。
別途、mapの2ページ目に実PTE衝突を起こし、1ページ目だけの巻き戻しと既存データ保持を確認しました。
生きた写像とオブジェクトを残したStoreのDropでも全回収を確認しています。
M2の本物のフレーム枯渇・回収試験も継続して通っています。

## 変更ファイル

- `src/object.rs`（新規）: 記述子、物理backing所有、Capability表、権利、map/unmap/release/revoke、モデルテスト。
- `src/m3.rs`（新規）: 実メモリbackend、HHDM seal、QEMU統合試験。
- `src/paging.rs`: 既存PTEの権利変更とTLB無効化、対応テスト。
- `src/m2.rs`、`src/main.rs`、`src/lib.rs`: M3接続と必要最小限の内部公開。
- `src/faults.rs`: M3用の例外ログラベル。
- `tools/qemu.py`、`tools/test_qemu.py`: M3ログ・会計・保護例外の検証と検証器テスト。
- `README.md`、`docs/ARCHITECTURE.md`、`docs/ROADMAP.md`、`docs/BUILD.md`、`docs/DECISIONS.md`: 現在の実装・境界を更新。
- `docs/M2-VERIFICATION.md`: 過去の生成物ハッシュである旨を追記。
- `docs/M3-VERIFICATION.md`、`docs/evidence/m3/`（新規）: 報告、シリアル、コマンド、終了コード、ハッシュ。

## 未実装・未検証と次のM4

M3の2つの論理タスク表は同一supervisorアドレス空間内です。ring-3のタスク切替、
独立アドレス空間、syscall境界、悪意あるユーザーコードからの隔離を実証したものではありません。
task引数は信頼されたカーネルだけが指定します。64 KiB/オブジェクト、4タスク表、
16 Capability/表、8オブジェクト、32写像という固定上限があります。

M4ではGDT/TSSとring-3移行、カーネルが呼び出し元を決定するsyscall、task別CR3とUser PTE、
タスク終了時の回収を実装します。実タスク2つによる共有・権限違反・失効を検証し、
次にCOWの権利、ページ単位の所有権、write faultでのコピー、alias管理を追加します。
4 MiBデモには固定ページ配列の拡張も必要です。

SMP・並行アクセス・TLB shootdown、COW、4 MiBの実タスクデモ、性能比較、圧縮、重複排除、
実機起動は今回未実装・未検証です。新規性や既存OSに対する性能優位性は主張しません。
