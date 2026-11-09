# Milestone 4 検証報告

この文書と `evidence/m4/` はM4時点の履歴です。通常の成果物はM7のビルドで更新されました。
現在のISO・ELF・検証結果は [M7検証報告](M7-VERIFICATION.md) を参照してください。

報告日: 2026-11-09 JST。M4の受け入れ条件を実装し、実QEMUのring-3タスクで検証しました。
独立したCR3、4 MiBの同一物理ページ共有、1ページCOW、権限拒否、失効と終了時の回収が動作しています。
実行ホストが記録したフォルダ日時は20261108です。証跡の名前・タイムスタンプは変更していません。

## 生成物と再実行

リポジトリのPowerShellで実行した正式コマンド:

```powershell
./tools/project.ps1 verify
```

最終実行は終了コード0、内部の全18コマンドも終了コード0です。
QEMU自身の終了コードは正常33、意図的panic／想定外fault35、タイムアウト試験-9です。
最後に正常版ELFへ戻し、両RAM構成で実際に起動した正常版ISOを保持しています。

| 生成物 | パス | SHA256 |
| --- | --- | --- |
| 正常版UEFI ISO | `build/normal/shared-first.iso` | `4c3ab1dc6185fca6f237a5be05b442a7b0f16efcdeef703e1f4d816c0511f6db` |
| 正常版Rust ELF | `target/x86_64-unknown-none/release/shared-first-kernel` | `6101613f58e5517a18c9d827726039d1e633d8d150591a334b74496e5467c20b` |

[生成物照合記録](evidence/m4/artifact-verification.json)に、元の実行フォルダ、18コマンドの成功、
最終ISOと両実起動ISOの一致、最終ELFとISO作成時ELFの一致を記録しました。
Rust 1.96.0、Limine v8.7.0の固定コミット、WSL UbuntuのQEMU 10.2.1 / OVMFを継続利用しています。
QEMUはq35・qemu64・TCG・1 CPU、ネットワークなし・ホスト物理ディスクなしです。
追加依存関係の導入、commit、push、branch作成は行っていません。

## 実施した検証

| 検証 | 最終結果 | 証跡 |
| --- | --- | --- |
| Rust整形 | 成功 | [fmt](evidence/m4/commands/02.log) |
| Rustホスト単体テスト | 30件成功 | [出力](evidence/m4/commands/03.log) |
| Clippy、警告をエラー化 | ホスト・カーネルとも成功 | [ホスト](evidence/m4/commands/04.log)、[カーネル](evidence/m4/commands/05.log) |
| Python検証器の単体テスト | 20件成功 | [出力](evidence/m4/commands/06.log) |
| Rust releaseビルドとISO作成 | 成功 | [ビルド](evidence/m4/commands/07.log)、[生成物ハッシュ](evidence/m4/image-normal/hashes.json) |
| QEMU 128 MiB | M1/M2/M3/M4 PASS、コード33 | [シリアル](evidence/m4/boot-128M/serial-readable.log)、[結果](evidence/m4/boot-128M/result.json) |
| QEMU 256 MiB | M1/M2/M3/M4 PASS、コード33 | [シリアル](evidence/m4/boot-256M/serial-readable.log)、[結果](evidence/m4/boot-256M/result.json) |
| 意図的panic | 想定どおりコード35 | [結果](evidence/m4/panic/result.json) |
| 想定外のカーネルページフォルト | 診断後コード35 | [結果](evidence/m4/unexpected-fault/result.json) |
| 実QEMUのタイムアウト | 2秒制限でkill・wait、コード-9 | [結果](evidence/m4/timeout/result.json) |

全コマンドの引数・cwd・終了コードは `evidence/m4/commands/*.json`、出力は対応する `.log` に保存しました。
QEMUの正確な引数・ISO／ファームウェアのハッシュは各 `result.json` にあります。
Pythonの合成ログ試験は検証器のテストであり、実起動の証拠とは区別しています。

初回Clippyで `collapsible_if` と `needless_range_loop` が失敗しました。
分岐とIDTのvector取得処理を修正し、最終の全検証を通しました。
失敗時の[出力](evidence/m4/failures/clippy.log)と[終了コード](evidence/m4/failures/clippy.json)も保存しています。

## 共有とCOWの実測

主タスクは別々のCR3で実CPL3を実行します。kernelが共有全1,024ページのPTEを照合し、
両方が同じ物理フレームをRO/NXで参照することを確認します。ユーザー自身も全4 MiBを読みます。
初期値0x5aのchecksumは両方377,487,360です。

書込み権COWを持つ側が同じページの2バイトを0xa7・0xa8に変更すると、write faultが1回発生し、
4 KiBだけをコピーします。書込み側のchecksumは377,487,515、読取り側は377,487,360のままです。
1,023ページは共有を維持し、元フレームの内容も変化しません。これはIPCによる全体コピーではありません。

| 項目（フレームは各4 KiB） | 128 MiB | 256 MiB |
| --- | ---: | ---: |
| usableプール総フレーム | 30,163 | 62,931 |
| M4開始前の割当て | 89 | 153 |
| 開始前ページテーブル | 73 | 137 |
| 共有payload | 1,024 | 1,024 |
| 共有ユーザー仮想ページ | 2,048 | 2,048 |
| コード・スタック・秘密ページ | 5 | 5 |
| 追加ユーザーページテーブル | 16 | 16 |
| 共有時の総割当て | 1,134 | 1,198 |
| COW後payload（うち私有コピー1） | 1,025 | 1,025 |
| COW後の総割当て | 1,135 | 1,199 |
| 終了後の総割当て | 89 | 153 |
| 終了後ページテーブル | 73 | 137 |
| 終了後payload・私有・タスク固有ページ・Capability | 全て0 | 全て0 |

M4 object storeの固定管理配列は331,584バイトです。カーネル画像の予約領域に含まれ、
usableプールの割当てには二重加算しません。これはCPU/runtimeを含む全メタデータ量ではありません。
TSS入口とdouble-fault用の各16 KiBスタックもカーネル画像に含まれます。
M3互換storeの管理配列は現行版で9,024バイトです。M3報告の4,928バイトは当時の値として保存しています。
file-backedページとcompressed bytesは未実装で0です。8 MiBの仮想写像合計は物理payload量ではありません。

## syscallと隔離試験

実ユーザーコードからmap / unmap / release / grant / revoke / whoami / yield / exit / createを呼びます。
呼び出し元偽装引数、未定義syscall、古い世代、WRITE要求、supervisor VA、権利増幅などを拒否します。
CREATEでは長さ0・初期化値256を拒否し、4,097バイト・0x33での作成後にユーザー自身が先頭・末尾・
ゼロ埋めを確認します。作成したCapabilityと写像を保持したまま正常終了し、kernelが回収します。

主タスクとは別のtask slotを再利用し、次の12ケースを実行します。各ケースの後で
フレーム・ページテーブル・写像数が開始前に戻り、主タスクのCapabilityが残ることを照合します。

| case | 動作 | 確認した結果 |
| --- | --- | --- |
| 0 | unmap／再map／release、CREATE、終了 | stale拒否、作成データ確認、status 0、全回収 |
| 1 | COW権のないRO共有ページへ書込み | #PF vector 14、error 7、タスク終了 |
| 2 | NX共有ページの実行 | #PF、error 21、タスク終了 |
| 3 | カーネル画像の読取り | #PF、error 5、タスク終了 |
| 4 | HHDM共有元の読取り | #PF、error 5、タスク終了 |
| 5 | 他タスクだけが持つ秘密ページの読取り | #PF、error 4、タスク終了 |
| 6 | ポート0xf4への直接I/O | #GP vector 13、error 0、QEMUを終了できずタスク終了 |
| 7 | DPL0のint 0x81 | #GP、error 1034、タスク終了 |
| 8 | 自分のRXコードへの書込み | #PF、error 7、タスク終了 |
| 9 | ユーザースタック前の未写像ページ読取り | #PF、error 4、タスク終了 |
| 10 | FPU初期化命令 | #NM vector 7、タスク終了 |
| 11 | COW権付き写像でコピー割当て失敗を注入 | 元データを壊さず#PFで終了、全回収 |

最後に主タスクからrevokeし、両タスクの2,048写像と私有コピーを除去します。
両タスクを再開するとstale handleのmapは拒否され、旧VAの実読取りも#PF error 4で終了します。
CPL3のまま古いTLBで読める状態を残さないことを確認しました。

両RAM構成とも `entries=142 syscalls=128 denied=87 cow_faults=1 killed=13 exited=1` です。
主タスク2つ＋12ケースで計14回のタスク起動です。COW成功の1例外と終了させた13例外は、
それ以前のM2/M3の保護プローブ計16回とは別に数えます。

ホストテストは独立ユーザー下位表、借用カーネル表の保護、PTE置換、COWの権利・コピー・失効、
割当て／置換失敗、wire handle、不正値、同じCapabilityの複数viewの独立性も検査します。
検証器はM4記録欠落、偽CPL／CR3、読取り側改変、コピー数、リーク、fault／CREATE／拒否記録欠落を拒否します。

## 未実装・未検証

- 単一CPU・IF=0の協調実行です。タイマー／プリエンプション、SMP、TLB shootdownは未実装です。
  ユーザーの無限ループに対する可用性は保証せず、実験全体は外側のQEMUタイムアウトで止めます。
- ユーザーコードは組込みの実機械語です。汎用ELFローダ、任意ユーザーポインタのcopy API、
  ファイル／ネットワークI/O、対話シェル、forkは未実装です。
- COWはview単位の私有スナップショットです。unmapで変更を破棄し、再mapは元のimmutable内容です。
  持続的な可変オブジェクトやfork互換の意味論ではありません。
- FPU/SIMDは使用拒否を確認しただけで、保存・復元して利用する機能はありません。
- double-fault用ISTは実装しましたが、実際のカーネルスタック枯渇・double fault・NMIは未検証です。
- COWメモリ不足は失敗注入です。自然発生のCOW OOMやタスク生成途中のOOM回収は実QEMUで誘発していません。
  物理allocator全体の枯渇・回収試験はM2の既存試験を今回も通しています。
- 実機、KVM、BIOS、Secure Boot、長時間運転、包括的fuzz／形式的証明は未検証です。
  全ての任意ネイティブコードに対する安全性や性能優位性を、この有限試験だけから主張しません。
- 重複排除、圧縮、適応ポリシー、既存OS手法に対する公平な比較測定はまだありません。

## 変更ファイル

Git管理がないため、今回作業したファイルを明示します。

- 新規: `src/m4.rs`、`src/user_cpu.rs`、`src/user_entry.S`、`src/user_program.S`。
- 更新: `src/main.rs`、`src/m2.rs`（M4起動）、`src/paging.rs`（ユーザー表・leaf置換）、
  `src/object.rs`（容量・wire handle・COW・タスク回収）。
- 更新: `tools/qemu.py`、`tools/test_qemu.py`（M4実起動検証と検証器のテスト）。
- 更新: `README.md`、`docs/ARCHITECTURE.md`、`docs/ROADMAP.md`、`docs/BUILD.md`、
  `docs/DECISIONS.md`、`docs/M1-VERIFICATION.md`、`docs/M2-VERIFICATION.md`、`docs/M3-VERIFICATION.md`。
- 新規: 本報告と `docs/evidence/m4/`。ビルド結果は `build/`、`target/`、元ログは `artifacts/`。

[ソースのSHA256一覧](evidence/m4/source-hashes.json)は最終文書更新後の入力スナップショットです。
既存のM1/M2/M3証跡を残し、現在の成果物への案内だけを更新しました。

## 次のM5

任意有効化できる重複排除／圧縮ポリシーの小さい実装から始め、同じ負荷の有効／無効A/B試験を作ります。
payload・管理データ・仮想写像・圧縮バイトを分離し、圧縮困難なデータでの損失や処理遅延も記録します。
共有元の不変性、Capability、COW、失効、フレーム寿命の既存試験を維持することが条件です。
タイマーなどの一般OS機能は別の残課題で、M5の比較結果と混同しません。
