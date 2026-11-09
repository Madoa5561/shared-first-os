# Milestone 9 検証報告

M9は外部バイトコードをLimine moduleとして読み込む有限の研究用ローダです。
M8の実行資源制限に、version付き入力・全件検証・所有コピー・code quotaを接続しました。
正式verifyの24コマンドが全て終了コード0で完了し、M9のゲートを通過しました。
形式と脅威モデルは [M9-FORMAT.md](M9-FORMAT.md) に記載します。

## 実装と変更ファイル

- `src/loader.rs`: 最大592 byteのparser、1..4program、各16..256 byte、code合計512 byte、
  fuel合計2,097,152以下。全件検証後だけBatchを返し、元inputへの依存を切る。
- `src/boot.rs` / `limine.conf`: Limine module要求・file ABI prefix、sizeとkind 6内の範囲確認。
- `src/m9.rs` / `src/m7.rs` / `src/lib.rs`: 所有ProgramをM8へadmitし、空のtask-local権限表から実行。
  HALT/faultでrelease_task。全Contextのborrow終了後にcodeをclearする。
- `tools/pack_bytecode.py` / `examples/bytecode/demo.json`: JSONから独立ファイルを生成。
- `tools/qemu.py` / `tools/project.ps1`: ISOへ格納し、同一ELFの外部入力違いを3ケース追加。
- `tools/m9_validation.py` / `tools/test_m9.py` / `tools/test_bytecode.py`: input fingerprint、
  実行結果・fuel・cleanupと拒否の独立照合、packer・検証器の負例。
- README、ARCHITECTURE、ROADMAP、BUILD、DECISIONS、形式/検証文書、M9証跡。

新しい依存、ホスト設定変更、物理ディスク操作はありません。nativeのCPL3/独立CR3を維持します。

## 検証手順

```powershell
./tools/project.ps1 verify
```

正式実行の記録は [commands](evidence/m9/commands) に保存します。
最初の開発checkではテスト配列のslice型変換、次にClippyのredundant pattern matchingで停止し、
それぞれ修正しました。失敗ログも [development-type](evidence/m9/development-type) /
[development-clippy](evidence/m9/development-clippy) に残します。

Rust単体は54件（M9追加5件）、Python検証器/packerは49件（追加7件）。
切断位置全件、末尾余剰、header/reserved/length、code/fuel合計、全未知opcode、最後の不正program、
元input変更後の所有コピーとclearをホストで検査しました。これらを実起動の代用にしていません。

## 実QEMUの正常結果

| 条件 | 実測 | 証跡 |
| --- | --- | --- |
| 通常128 MiB | M1〜M9 PASS、code 33、74.517秒 | [result](evidence/m9/boot-128M/result.json) |
| 通常256 MiB | M1〜M9 PASS、code 33、64.684秒 | [result](evidence/m9/boot-256M/result.json) |
| 外部codeの42→99 | 同一ELF、M1〜M9 PASS、code 33、76.123秒 | [result](evidence/m9/module-alt/result.json) |
| 不正版番号 | Version拒否、admitted=0、code 33 | [result](evidence/m9/module-bad-version/result.json) |
| 最後の不正opcode | Code(Opcode)拒否、admitted=0、code 33 | [result](evidence/m9/module-bad-opcode/result.json) |
| 2秒timeout | kill/reap、code -9 | [result](evidence/m9/timeout/result.json) |
| 意図的panic | 診断・停止、code 35 | [result](evidence/m9/panic/result.json) |
| 想定外page fault | 診断・停止、code 35 | [result](evidence/m9/unexpected-fault/result.json) |

不正moduleのcode 33は想定した拒否を経て停止したことを表し、M9 PASSは出力しません。
検証器が拒否理由・admitted=0・M9TASK/M9LOADなしを照合して成功と判定しています。

通常moduleは224 byte / SHA256 `e5787c6385bacc7c11e25ac1316a884e2727f37bb6cfa7dc9822037cf8694984`。
alternate moduleはSHA256 `7a73511169ece402ead5a606ebbe896f3d1248cf5f407ea30e3ce587af1de3b6`。
両ISOのkernel SHA256は `85550d061f94743daea7dbb7dd039d558b9ecc4200bc0302e79d86a4aacf1849` で一致します。
別ファイルの内容が実行結果へ反映された証拠であり、カーネルに結果を埋め込んだ試験ではありません。

正常ISOは `build/normal/shared-first.iso`、SHA256は
`fb158479a06747099a2a1968efecbe1537484e7655541cb4ba8f3239b2a64be4`。
正常ELFはverifyの最後に復元済みで、起動したISO内ELFとのhash一致を確認しています。
[証跡照合](evidence/m9/artifact-verification.json) と [source hashes](evidence/m9/source-hashes.json) に保存しました。
CLI packerも実行し、ISO内のmoduleとhash一致、出力先が既存なら終了1で上書きを拒否し既存内容が
不変であることを [cli結果](evidence/m9/packer/m9-pack-cli-result.json) に記録しています。

通常の3programは次の結果でした。

1. CONST/HALTで42を返却。初期fuel 4→2。
2. 1+2を計算し、quantumとYIELDをまたいで3を返却。fuel 5→3→1→0。
3. 数値handle 65536でSUMを試み、空のCapability表でAuthority拒否。fuel 8→5。

5dispatch・3stop後、code_bytes=0/task_count=0。allocatorは128 MiBで89→89、256 MiBで153→153、
ページテーブルは73/137で不変。仮想写像数は31,393/64,161で不変。capabilities/payloadは0です。
全正常・module変更のbootでM4の4 MiB共有とCOW、M5の18試行、M6の198試行、M7の22攻撃と36試行、
M8の17dispatchも回帰検証しています。

## 会計と限界

有効codeは160 byte、所有Batch構造体は1,096 byte（1,024 byteの固定code予約を含む）、
Schedulerは608 byte。既存64 KiB stackにあり、実stack最高使用量は未測定です。
カーネル画像876,544 byte（M8より12,288 byte増）、元moduleは予約1ページ4,096 byteを保持。
`MEMKIND kernel_and_modules`は2領域880,640 byteで、画像と元moduleの合計と一致します。
Batch.clearは実行用コピーの論理解放で、元moduleの予約ページを解放する操作ではありません。
管理配列を画像/stackへ二重加算せず、RAM削減や性能優位性は主張しません。

入力由来のcodeとmetadataは固定上限で検証しますが、Limineのメタデータ/ポインタは信頼します。
任意native/ELF/WASM、署名検証、汎用ファイルシステム、動的ロード、SMP、native preemption、
元moduleの物理ページ回収、実機動作、安全性の形式証明は未実装/未検証です。

次は信頼されたlaunch manifestで初期Capabilityを付与し、外部プログラム同士がimmutable objectを
共有するデモを作るのが適切です。同時にscheduler/authority hostの所有関係と終了cleanupを
型で固定し、初期権限を外部code自身に決めさせない境界を維持します。
