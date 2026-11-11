# Milestone 14 検証報告

M14は外部codeから実行と資源回収までをまとめる同期kernel APIです。
[M14-DESIGN.md](M14-DESIGN.md) に境界を定義します。正式verifyの33コマンドが全て終了0となり、ゲートを通過しました。

## 変更ファイルとホスト検査

- `src/runner.rs` / `src/lib.rs`: RunOptions、StartError、Completion、Reportとexecute。
- `src/launch.rs`: 既存テスト用backendで起動APIの正常/取消/失敗/再試行の4単体テスト。
- `src/m14.rs` / `src/m7.rs`: 新APIだけで外部moduleを実行する8caseの実QEMU試験。
- `tools/m14_validation.py` / `tools/test_m14.py` / `tools/qemu.py`: 終了状態・拒否段階・物理会計の独立照合。
- README、ARCHITECTURE、ROADMAP、BUILD、DECISIONS、M14設計/本報告、証跡。

正式verifyでRust74件（追加4）、Rustdoc3件、Python63件（追加3）、fmt、host/kernel Clippyが合格。
単体では通常終了、上限と同時の全終了、途中取消、0回実行、設定範囲外、最後のopcode不正、
正しい形式だがmanifestと異なる件数、busy storeの権限/データ保持、allocate/seal失敗と再試行を検査。
Reportの会計は準備失敗でも復元され、busy時には既存資源が維持されます。
新しい依存・host設定・bytecode命令・module・物理ディスクへのインストールはありません。

```powershell
./tools/project.ps1 verify
```

## QEMU結果（全件合格）

正確なコマンド・出力・終了コードは [commands](evidence/m14/commands) に保存しています。

| 条件 | 確認結果 | 証跡 |
| --- | --- | --- |
| 通常128 MiB | M1〜M14 PASS、終了33、52.532秒 | [result](evidence/m14/boot-128M/result.json) |
| 通常256 MiB | M1〜M14 PASS、終了33、53.637秒 | [result](evidence/m14/boot-256M/result.json) |
| module-alt | M9外部値99、M14成功 | [result](evidence/m14/module-alt/result.json) |
| module-bad-version | M9拒否、M14成功 | [result](evidence/m14/module-bad-version/result.json) |
| module-bad-opcode | M9拒否、M14成功 | [result](evidence/m14/module-bad-opcode/result.json) |
| shared-alt | M11読取り長変更、M14成功 | [result](evidence/m14/shared-alt/result.json) |
| shared-bad-opcode | M11拒否、M14成功 | [result](evidence/m14/shared-bad-opcode/result.json) |
| shared-bad-count | M11拒否、M14成功 | [result](evidence/m14/shared-bad-count/result.json) |
| revocation-bad-opcode | M12/M13拒否、M14の実行4caseもLoad拒否、8caseの検査成功 | [result](evidence/m14/revocation-bad-opcode/result.json) |
| 意図的2秒timeout | kill/reap、終了-9 | [result](evidence/m14/timeout/result.json) |
| 意図的panic | 診断・終了35 | [result](evidence/m14/panic/result.json) |
| 想定外page fault | 診断・終了35 | [result](evidence/m14/unexpected-fault/result.json) |

通常boot上限180秒。失敗経路の合格は検出器の判定であり、正常bootとは区別します。

第三module `revocation.sfbc` に対してquantum1、上限16/9/2/0を実行します。
上限16と9は2taskがHalted(28672)、無権限taskがFaulted(Authority)、9dispatch、limited=false。
残燃料は4/4/4103。上限2は残燃料4103/4103/8200を持って全task取消、上限0は8200ずつで取消。
実行taskのstopは各1回。API返却時にはownerも消え、今回取得したobject/payload/capabilityは0。

残り4caseはquantum0、busy store、入力version2、有効な2program（固定manifestは3を要求）を検査。
busy時には既存ownerの権限と36864の読取り値を維持し、試験側だけが最後に解放します。
通常実行の成功read4回と途中取消の2回は同じ実frameで、RO aliasを既存Hardware backendが検査します。
各caseのframe/table/mapping開始・終了収支とピークを照合します。

revocation-bad-opcodeでは前半4caseがLoad(Code(Opcode))で拒否され、実行や権限準備はしません。
busy caseもcode検証が先なのでLoad拒否ですが既存資源を保ちます。別途version/count拒否も確認。
この場合のM14 PASSは拒否・回収試験の合格であり、外部codeの正常実行を意味しません。

## 会計と成果物

全caseのallocator収支は128 MiBで89→89、256 MiBで153→153。
objectを取得するcaseとbusy fixtureのピークはそれぞれ90/154、事前拒否だけのcaseは89/153。
table数73/137、virtual mapping31,408/64,176も開始終了で一致しました。
読取りframeは128 MiBで0x5a000、256 MiBで0x9a000。各case内で許可されたreadは同一backingです。

Report272 byte（Completion176 byteを含む）、Batch1,096 byte、Session648 byte、Launch88 byte。
固定stack上の値で、payloadと二重加算しません。実stack高水位は未測定。
カーネル予約画像領域937,984 byte（M13比+16,384）、保持する3module予約ページ12,288 byte、
kernel_and_modules合計950,272 byte。RAM削減や速度改善は主張しません。

通常ISO SHA256: `d49cc47ad5d1c64433d4e3df1badf28e799915e024f49d47d6fca5df5c3d5e53`。
通常ELF SHA256: `6a09d4d772390af56de812ce2c6e204bfe7137d03663fd0c2b10821f000c0255`。
3moduleはM13と同じ内容。[artifact verification](evidence/m14/artifact-verification.json) に
最終ELF/ISO/module・証跡コピーのハッシュと、7variantの同一ELF照合を保存します。
[source hashes](evidence/m14/source-hashes.json) はソース・設定・文書の対応表です。
ISO日時固定によるbit単位の再現性は主張しません。CIはportable Rust/Python検査で、QEMUはローカルWindows/WSLの結果です。

## 未検証と次段階

固定3program・kernel内READ manifest・信頼された呼出し元を前提とする同期APIです。
dispatch上限は実時間上限ではなく、host callは途中中断しません。途中の外部制御・再開可能job管理はありません。
コードclearは論理的消去で、最適化後のsecure eraseやstack高水位は未検証です。
後処理の保証は通常returnに限り、abort/電源断/mem::forgetを含みません。
Session生成失敗の分岐は現在のBatch quotaと事前設定検査では到達せず、防御的に保持しています。
この分岐への故障注入は未実施です。allocate/seal失敗の注入はhost単体試験で、実QEMUでの注入ではありません。
SMP、native preemption、動的slot再利用、実機、署名認証、形式証明は対象外です。
次は起動APIへ生成packageと異常入力を与える体系的なモデル照合で、予算・失敗・回収の検査を広げます。
