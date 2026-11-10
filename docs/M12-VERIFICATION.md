# Milestone 12 検証報告

M12はSessionのhost固定を保ったまま、信頼された制御要求で共有objectを失効させます。
中断中の外部bytecodeを再開して拒否し、同じstoreで再起動して旧handle拒否・新handle成功を検査します。
正式verifyの33コマンドが全て終了0となり、M12のゲートを通過しました。
[M12-DESIGN.md](M12-DESIGN.md) に設計境界を記載します。

## 変更ファイルとホスト検査

- `src/session.rs`: RevocableHost限定のrevoke制御。host参照の公開・差替え・燃料補充は行わない。
- `src/launch.rs`: 専用owner handleの保持と同期失効、重複要求false、引数の論理clear、3追加単体テスト。
- `src/m12.rs` / `src/m7.rs`: 3caseの実QEMU検査とシリアル記録。
- `src/boot.rs` / `limine.conf`: 正確に3個のboot moduleを受理し、各index/size/retained領域を検査。
- `examples/bytecode/revocation.json`: SUM/YIELD/SUM/HALTの3外部program。code192 byte、fuel合計24,600。
- `tools/qemu.py` / `tools/project.ps1`: 3個目のmodule生成、revocation-bad-opcode、全33コマンド。
- `tools/m12_validation.py` / `tools/test_m12.py`: 失効・read frame・fuel・世代・終了収支の独立照合。
- `tools/m11_validation.py` / `tools/test_m11.py`: Launch管理サイズを実測88 byteへ更新。
- README、ARCHITECTURE、ROADMAP、BUILD、DECISIONS、形式/設計/本報告、証跡。

追加依存・ホスト設定変更・物理ディスクへのインストールはありません。
Rust単体66件（追加3）、Rustdoc compile tests 3件、Python57件（追加3）、fmtとhost/kernel Clippyが合格しました。
単体テストは中断時の失効とfuel維持、cancel+revoke+Dropのstop各1回、再起動後のstale/foreign拒否を含みます。

```powershell
./tools/project.ps1 verify
```

正確なコマンド・出力・終了コードは [commands](evidence/m12/commands) に保存します。
通常bootは180秒上限、意図的なtimeout試験は2秒。全ケースを省略せず実行します。

## 実QEMUの最終結果（全件合格）

| 条件 | 確認した結果 | 証跡 |
| --- | --- | --- |
| 通常128 MiB | M1〜M12 PASS、終了33、48.268秒 | [result](evidence/m12/boot-128M/result.json) |
| 通常256 MiB | M1〜M12 PASS、終了33、49.716秒 | [result](evidence/m12/boot-256M/result.json) |
| module-alt | M9の外部値99、M12全case成功 | [result](evidence/m12/module-alt/result.json) |
| module-bad-version | M9拒否、M12成功 | [result](evidence/m12/module-bad-version/result.json) |
| module-bad-opcode | M9拒否、M12成功 | [result](evidence/m12/module-bad-opcode/result.json) |
| shared-alt | M11読取り長変更、M12成功 | [result](evidence/m12/shared-alt/result.json) |
| shared-bad-opcode | M11 code拒否、M12成功 | [result](evidence/m12/shared-bad-opcode/result.json) |
| shared-bad-count | M11 manifest拒否、M12成功 | [result](evidence/m12/shared-bad-count/result.json) |
| revocation-bad-opcode | 最後のcode拒否、admitted=0/grants=0、M12 PASSなし | [result](evidence/m12/revocation-bad-opcode/result.json) |
| 意図的2秒timeout | kill/reap、終了-9 | [result](evidence/m12/timeout/result.json) |
| 意図的panic | 診断・終了35 | [result](evidence/m12/panic/result.json) |
| 想定外page fault | 診断・終了35 | [result](evidence/m12/unexpected-fault/result.json) |

M12正常時は同じ外部codeの所有Batchを3caseで再利用します。

1. task 0/1が同じ1frameを読んで28,672を返しYIELD。fuel 8,200→4,102。
   revokeでpayload/capabilityを0にし、二度目の要求はfalse。無権限task 2を拒否し、
   再開したtask 0/1もAuthority faultとなります。再読取りのfuelは4,102→5で、補充されません。
2. 同じYIELD後にtask 0をcancelし、再cancelはfalse。revoke後task 2/1を拒否。全taskのstopは各1回。
3. 新Launchのtask 0へcase 0の旧handle、task 1へ新handle、task 2へtask 1の数値を渡す。
   task 0/2は拒否、task 1だけが2回のSUMに成功し、fuel 4,102→4でHALTします。

失効直後のpoolは128 MiBで90→89。重複要求は89→89。正常終了を待たずにpayloadを戻します。
case 2ではowner参照1を残して全taskを停止し、LaunchのDropで最後に回収します。
各caseの終了収支・stop数は独立検証器が照合し、許可されたreadの実frame一致とRO PTEも検査します。
拒否されたreadはbackendに到達せずframe/pagesとも0。M12はインタプリタ経由で、native PTE隔離の代替ではありません。

## 会計と成果物

Launchは88 byte（ownerのOption保持でM11の80 byteから8 byte増）、Session616 byte、Batch1,096 byte。
固定stack内の管理領域で、物理payloadに二重加算しません。stack高水位は未測定です。
128 MiBの各caseはallocator89→89、ピーク90、table73、virtual mapping31,401で開始/終了一致。
256 MiBでもallocator153→153、ピーク154、table137、virtual mapping64,169で開始/終了一致しました。
カーネル画像909,312 byte（M11より16,384 byte増）。3つの元module予約ページ12,288 byteは保持し、
`kernel_and_modules`合計921,600 byteです。payload回収を元moduleページの解放とは扱いません。

ISO/ELF/3moduleと証跡コピーのSHA256、7variantの同一ELFは
[artifact verification](evidence/m12/artifact-verification.json)、ソース/設定/文書は
[source hashes](evidence/m12/source-hashes.json) で照合します。ISO日時は固定していません。
配布する通常ISOのSHA256は `d1ecace0314a517892db9f08f57fbe686aca276a0bdcc4647d9204891f1321db`、
正常ELFは `82e60c59ee484875f29b53819b062e3b25369c671118e2084e81e03a6a6a604d`。
M12の256-byte moduleは `d2e5eff877833b96fe9a8f2a256c556a99ea82af8bcdcebd59b11b6131f0808b`。
追加機能によるRAM削減や速度改善は主張しません。

## 未検証と次段階

失効は今後のobject読取りを拒否し、既にregister/scratchへ読んだ値を消しません。
制御要求・kernel manifest・boot module順序・hostを信頼します。署名・code同一性の認証はありません。
単一CPUの同期host操作のみで、SMP時の並行失効/TLB shootdown、native preemption、汎用再起動管理、
動的slot再利用、実機、形式証明、panic=abort/mem::forget時のDrop保証は対象外です。
既存M4のCPL3/独立CR3と失効後fault、M5〜M11の回帰試験を維持します。

次は停止理由・資源残量を取得できる有界の実行管理APIへ進め、準備・検証・実行・取消・失効の
状態遷移と失敗処理を、固定fixture外からも一貫して扱えるようにします。
