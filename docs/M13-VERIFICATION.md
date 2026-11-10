# Milestone 13 検証報告

M13は終了理由・残燃料・共有資源数を、実行中のSessionからコピーで取得するkernel用APIです。
[M13-DESIGN.md](M13-DESIGN.md) に範囲を定義します。正式verifyの33コマンドが全て終了0となり、ゲートを通過しました。

## 変更ファイルとホスト検証

- `src/scheduler.rs`: TaskState/TaskSnapshot/Snapshot、終了理由の保持、読み取り専用照会、3単体テスト。
- `src/session.rs`: Resources/InspectableHost/Inspection、snapshot/inspect。host借用は維持。
- `src/launch.rs`: 集約資源数・失効状態の照会、取消/失効/終了後の状態検査。
- `src/m13.rs` / `src/m7.rs`: 外部codeを1命令quantumで動かす実QEMUの2case。
- `tools/m13_validation.py` / `tools/test_m13.py` / `tools/qemu.py`: 全状態・燃料・実read・資源会計の独立照合。
- `tools/m9_validation.py` / `tools/test_m9.py` / `tools/m10_validation.py` / `tools/test_m10.py`: 現在の管理サイズを更新。
- README、ARCHITECTURE、ROADMAP、BUILD、DECISIONS、M13設計/本報告と証跡。

新しい依存、host設定、bytecode命令、module、物理ディスクへのインストールは追加していません。
正式verifyでRust70件（追加4）、Rustdoc3件、Python60件（追加3）、fmt、host/kernel Clippyが合格。
照会の有無で同じdispatch結果と停止回数、取得copyを変更しても実体が不変、拒否時の不変性、
HALT/Authority/Fuel/取消理由、YIELD/quantum満了後のReady、未使用slotを検査しました。

```powershell
./tools/project.ps1 verify
```

## QEMU検証結果（全件合格）

正確なコマンドと出力・終了コードは [commands](evidence/m13/commands) に保存しました。

| 条件 | 確認結果 | 証跡 |
| --- | --- | --- |
| 通常128 MiB | M1〜M13 PASS、終了33、51.829秒 | [result](evidence/m13/boot-128M/result.json) |
| 通常256 MiB | M1〜M13 PASS、終了33、53.225秒 | [result](evidence/m13/boot-256M/result.json) |
| module-alt | M9外部値99、M13成功 | [result](evidence/m13/module-alt/result.json) |
| module-bad-version | M9拒否、M13成功 | [result](evidence/m13/module-bad-version/result.json) |
| module-bad-opcode | M9拒否、M13成功 | [result](evidence/m13/module-bad-opcode/result.json) |
| shared-alt | M11読取り長変更、M13成功 | [result](evidence/m13/shared-alt/result.json) |
| shared-bad-opcode | M11拒否、M13成功 | [result](evidence/m13/shared-bad-opcode/result.json) |
| shared-bad-count | M11拒否、M13成功 | [result](evidence/m13/shared-bad-count/result.json) |
| revocation-bad-opcode | M12/M13拒否、admitted=0/grants=0、PASSなし | [result](evidence/m13/revocation-bad-opcode/result.json) |
| 意図的2秒timeout | kill/reap、終了-9 | [result](evidence/m13/timeout/result.json) |
| 意図的panic | 診断・終了35 | [result](evidence/m13/panic/result.json) |
| 想定外page fault | 診断・終了35 | [result](evidence/m13/unexpected-fault/result.json) |

通常bootの上限は180秒。失敗経路の成功は検証器の判定であり、正常bootとは区別します。

外部`revocation.sfbc`を全件検証してLaunchを準備し、同じBatchで2caseを実行します。
case 0は9dispatchで2readerがHALT、無権限taskがAuthority fault。11snapshotを保存。
case 1は2readerのSUM直後にtask 0をcancelしobjectをrevoke、残るtaskは通常の認可で拒否。
途中のYIELDは既読値を返せるが新規読取りはできません。9snapshotを保存。
合計20snapshot、80slot観測、14dispatch、9readをPython側で独立照合します。
revokeでtaskの状態/残燃料は変わらず、後のSUM時にFaultedへ遷移することも検査します。
未使用slot 3はEmpty、終了後cancel/dispatchで過去の終了理由を上書きしません。

管理サイズはScheduler640 byte、Session648 byte（各M12比+32）、Snapshot128 byte、Inspection168 byte。
snapshotは要求時に返す値で、ヒープ確保はありません。固定stack内の管理領域と実payloadを別に扱います。
最適化後の実stack高水位・照会の時間コストは未測定です。

確認済み通常bootは128 MiBで51.829秒、256 MiBで53.225秒、どちらもM1〜M13 PASS・終了33。
両caseのallocator収支は128 MiBで89→89（ピーク90、table73、mapping31,404）、
256 MiBで153→153（ピーク154、table137、mapping64,172）。各taskのstopは1回です。
case 0の全task終了時はowner capability1とpayload1を維持し、LaunchのDropで回収します。
case 1のrevoke直後はobject/payload/capabilityが0、残るtaskのReadyと残燃料はそのままです。

カーネルの予約画像領域は921,600 byte（M12比+12,288）。3moduleの予約ページ12,288 byteは保持し、
kernel_and_modules合計933,888 byte。仮想mapping数や固定stack内のcopyをpayloadへ二重加算しません。
通常ISO SHA256: `1b1c2ab6b88738cdedbfa6f7e39ec2de01d2002834ca02c4af2a76991d8b5ee5`。
通常ELF SHA256: `d80f02416ca9a71495af88699c53c42bb6726757523bd8fc64247de74b6ae59f`。
外部3moduleはM12と同一内容です。ISO日時の固定によるbit単位の再現性は主張しません。

[artifact verification](evidence/m13/artifact-verification.json) に最終ELF/ISO/3moduleと証跡コピーの
SHA256、7入力variantの同一ELF照合を記録します。[source hashes](evidence/m13/source-hashes.json) は
ソース・設定・文書の対応表です。CIはportable Rust/Python検査で、QEMUは上記ローカルWindows/WSLの結果です。

## 未検証と次段階

APIは信頼されたkernel向けで、bytecode自身やnative taskからの状態照会syscallではありません。
InspectableHostの副作用なし/非再入は信頼契約。単一CPUの同期操作間だけが対象です。
結果copyは取得時点の値で自動更新されず、残燃料合計には停止済みtaskの再利用できない未使用分を含みます。
失効は既読値を消しません。物理RAM削減や速度改善は主張しません。
SMP、native preemption、動的slot再利用、汎用起動manager、実機、形式証明は対象外です。
次はコード検証・権限準備・実行のエラーと回収結果をまとめる有界の起動入口です。
