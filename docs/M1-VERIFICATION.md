# Milestone 1 検証報告

> M1完了時点の保存記録です。現在のソースと生成物はM7へ進んでいるため、
> 最新の検証とISOハッシュは [M7検証報告](M7-VERIFICATION.md) を参照してください。

2026-11-08 JST。M1のゲートを通過しました。Rust `no_std` の独自x86-64カーネルを
ビルドし、Limine + OVMFから実際にQEMUで起動しています。

## 最終生成物

| 生成物 | パス | SHA256 |
| --- | --- | --- |
| 正常起動用UEFI ISO | `build/normal/shared-first.iso` | `f0360dc88325ddce8fab44ef8ab99ca587adf8750330bbf1c556299b7e2b5a78` |
| RustカーネルELF | `target/x86_64-unknown-none/release/shared-first-kernel` | `b5ac89be167660409bf7bed85bcca5df1646587c01b2ec44816f0598bc142b5b` |

この正常版ISOのハッシュは、以下の128 MiB・256 MiB試験の `result.json` と一致します。
パニック試験後は正常版ELFに戻し、検証済みISOは再生成せず保存しています。
再ビルド時にはISO内の日時等によってハッシュが変わり得ます。

## 実行コマンドと結果

Windows PowerShell、リポジトリのルートで実行しました。

```powershell
./tools/project.ps1 verify
```

一括検証の終了コードは **0** です。内部の全15コマンドも終了コード0です。
QEMUプロセス自身のコードは検証器が別途保存・判定しています。
正確な引数配列・作業ディレクトリ・終了コード・出力は
[commands](evidence/m1/commands/) に保存しています。

| 検証 | 結果 | 証跡 |
| --- | --- | --- |
| `cargo fmt --check` | 成功 | [出力](evidence/m1/commands/02.log) |
| Rustホスト単体テスト | 5件成功、0件失敗 | [出力](evidence/m1/commands/03.log) |
| Clippy ホスト / カーネル、警告をエラー化 | 両方成功 | [ホスト](evidence/m1/commands/04.log)、[カーネル](evidence/m1/commands/05.log) |
| Python検証器単体テスト | 7件成功、0件失敗 | [出力](evidence/m1/commands/06.log) |
| releaseカーネルビルド | 成功 | [出力](evidence/m1/commands/07.log) |
| UEFI ISO作成 | 成功 | [xorrisoコマンド](evidence/m1/image-normal/command.json) |
| QEMU、RAM 128 MiB | 実起動成功、終了コード33 | [生シリアル](evidence/m1/boot-128M/serial.log)、[結果](evidence/m1/boot-128M/result.json) |
| QEMU、RAM 256 MiB | 実起動成功、終了コード33 | [生シリアル](evidence/m1/boot-256M/serial.log)、[結果](evidence/m1/boot-256M/result.json) |
| 意図的Rustパニック | パニックメッセージ確認、終了コード35 | [シリアル](evidence/m1/panic/serial-readable.log)、[結果](evidence/m1/panic/result.json) |
| 実QEMUを `-S` で停止した試験 | 2秒後にkill・wait完了、コード-9 | [結果](evidence/m1/timeout/result.json) |
| `readelf -h -l -W` | ELF64 / x86-64 / EXEC、4 LOADセグメント | [出力](evidence/m1/elf.log)、[終了コード](evidence/m1/elf.json) |

Python単体テストの合成ログを起動証拠としては使用していません。
タイムアウト試験の「成功」は停止したQEMUの検出・回収成功であり、OS起動成功ではありません。
Clippyはホストの集計テストと、全機能を有効にしたベアメタルカーネルの両方に対して実行しました。

## 実シリアル出力からの抜粋

128 MiBの正常版です。全39領域の出力は上記の生ログにあります。

```text
Shared-First OS booted
BOOT base_revision=3 firmware=2
MEMKIND name=usable regions=14 bytes=125349888
MEMKIND name=reserved regions=10 bytes=13161549824
MEMKIND name=bootloader_reclaimable regions=7 bytes=2265088
MEMKIND name=kernel_and_modules regions=1 bytes=32768
MEMSUMMARY entries=39 usable_bytes=125349888 reserved_bytes=13170102272 described_bytes=13295452160
ACCOUNTING reserved=all_non_usable; address_map_bytes_are_not_ram_usage; allocator=absent
M1 PASS
```

256 MiBでは `usable_bytes=259567616` でした。
差は134,217,728バイト（128 MiB）で、設定したRAMの増加と一致します。
予約領域にはMMIOのアドレス範囲が含まれるため、`reserved_bytes` は消費RAM量ではありません。
数値はこの環境・起動条件での観測値であり、ブートローダの配置変更で変わります。

## 使用環境

- Rust 1.96.0、MSVCホスト、LLVM 22.1.2、追加ターゲット `x86_64-unknown-none`。
- Limine v8.7.0 binary commit `aad3edd370955449717a334f0289dee10e2c5f01`。
- WSL2 Ubuntu 26.04、QEMU 10.2.1、OVMF 2025.11-3ubuntu7、Python 3.14.4。
- QEMUは `q35` / `qemu64` / `tcg` / CPU 1個、ネットワークなし。
- [実環境のパッケージ版](evidence/m1/environment.log)、[固定入力](../tools/versions.json)。

Windows側にはRustのOS用ターゲットを追加し、WSLにQEMU・OVMF・xorrisoと
その依存パッケージを、ユーザー承認後に導入しました。binutilsは既存版を使用しました。
Limine取得先はこのプロジェクト内です。ホストの物理ディスク・起動設定は変更していません。

## 修正した問題

1. 指定されていた `docs/ARCHITECTURE.md`、`docs/ROADMAP.md` が存在せず、
   ルート直下にあったため、原本を保存した上で `docs/` の正本と導線を追加しました。
2. 初回ISOは起動できましたが、Limineが起動ボリューム特定をフォールバックする警告を
   出しました。UEFIパーティション情報をISOに追加し、最終実起動では警告が消えました。
3. 一括検証の最後にISOを作り直すと、納品ISOと実際に試験したISOのハッシュが変わるため、
   最後は正常版ELFのみ戻し、試験済みISOを保持するようにしました。

Rustのビルド失敗やQEMUのカーネル起動失敗は発生していません。
初回と中間の生ログも `artifacts/` に残してあります。

## 追加・変更ファイル

| 分類 | ファイル |
| --- | --- |
| カーネル | `src/main.rs`、`src/boot.rs`、`src/serial.rs`、`src/machine.rs` |
| メモリ集計・ホストテスト | `src/lib.rs`、`src/memory.rs` |
| ビルド・起動設定 | `Cargo.toml`、`Cargo.lock`、`.cargo/config.toml`、`linker.ld`、`limine.conf`、`.gitignore` |
| 再現・検証ツール | `tools/project.ps1`、`tools/qemu.py`、`tools/test_qemu.py`、`tools/fetch-limine.ps1`、`tools/versions.json` |
| 既存文書の更新 | `README.md`、ルートの `ARCHITECTURE.md`、`ROADMAP.md` |
| 文書追加 | `docs/ARCHITECTURE.md`、`docs/ROADMAP.md`、`docs/BUILD.md`、`docs/DECISIONS.md`、この報告書、`THIRD_PARTY.md` |
| 保存証跡 | `docs/evidence/m1/` のシリアル、実行結果、環境・ELF記録 |

`AGENTS.md` と `codex-start.md` は読み取りのみです。
作業開始時点でGitリポジトリは初期化されておらず、init / branch / commit / pushは行っていません。

## 未実装・未検証と次のM2

M1で実証したのはQEMUのUEFI起動・シリアル出力・物理メモリマップ集計・試験制御です。
BIOS、Secure Boot、実機、KVM、SMP、Linux側のRustコンパイルは未検証です。
割当器、独自ページテーブル・例外処理、ring-3タスク、Capability、共有物理ページ、
COW、失効、圧縮、性能改善は未実装で、それらの安全性や効果を主張していません。

次のM2では、usable領域からのみ割り当てる4 KiBフレーム管理を実装します。
整列・枯渇・予約領域除外・二重割当て／二重解放・再利用をホストとQEMUで検証し、
その後にページテーブルのmap/unmap、TLB無効化、W^X、スタックガードへ進みます。
bootloader-reclaimableメモリは、応答・スタック・GDT・ページテーブルへの参照が
解消されるまで再利用しません。詳細は [ロードマップ](ROADMAP.md) を参照してください。
