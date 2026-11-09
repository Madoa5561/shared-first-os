# Milestone 6 検証報告

この文書と `evidence/m6/` はM6時点の履歴です。通常の成果物はM7のビルドで更新されました。
現在のISO・ELF・検証結果は [M7検証報告](M7-VERIFICATION.md) を参照してください。

2026-11-09 JST。M6のベースラインとベンチマーク報告のゲートを通過しました。
private、通常共有、通常COW、Capability object共有、object COWを実装し、同じ機械語を
独立したページテーブルの実ring-3タスク2つで実行しました。128／256 MiBの両構成で検証済みです。

共有とCOWによるpayload削減は通常方式でも同じでした。全ページを書き換える負荷では
COWのピークpayloadはprivateと同じになり、更新区間のfault処理コストが増えます。
Capability方式だけの新しいメモリ削減、既存OSへの性能優位性を示す結果ではありません。

## 再実行と生成物

```powershell
./tools/project.ps1 verify
```

全体の終了コード0、内部の18コマンドも全て0です。QEMU自身は正常33、意図的panicと
想定外fault35、2秒タイムアウト-9です。検証器は各ケースの期待値を照合します。
最後に通常版ELFを復元し、両構成で実起動した正常版ISOを残しました。

| 生成物 | パス | SHA256 |
| --- | --- | --- |
| UEFI ISO | `build/normal/shared-first.iso` | `e4e2a51b12c75db6334d78d957be831ae08ff584678e71b02231218314cf401e` |
| Rust ELF | `target/x86_64-unknown-none/release/shared-first-kernel` | `aebe9f6f40ea80e59432a99fec921d6f6da7b9140d60e512e1ccfdc17999eeeb` |

[生成物照合記録](evidence/m6/artifact-verification.json)に元証跡の場所と現在のファイルとの一致を保存しました。
Rust 1.96.0、Limine v8.7.0固定コミット、WSL Ubuntu、QEMU 10.2.1 / OVMFを継続利用します。
q35 / qemu64 / TCG / 1 CPU、ネットワークなし、ホスト物理ディスク接続なしです。
依存関係の追加、commit、push、branch作成はありません。

## 検証結果

| 検証 | 結果 | 証跡 |
| --- | --- | --- |
| Rust fmt | 成功 | [出力](evidence/m6/commands/02.log) |
| Rustホスト単体テスト | 37件成功 | [出力](evidence/m6/commands/03.log) |
| Clippy、警告をエラー化 | ホスト・全機能カーネルとも成功 | [ホスト](evidence/m6/commands/04.log)、[カーネル](evidence/m6/commands/05.log) |
| Python検証器 | 32件成功 | [出力](evidence/m6/commands/06.log) |
| Rust release／ISO | 成功 | [ビルド](evidence/m6/commands/07.log)、[ハッシュ](evidence/m6/image-normal/hashes.json) |
| QEMU 128 MiB | M1〜M6 PASS、exit 33、約50.72秒 | [serial](evidence/m6/boot-128M/serial-readable.log)、[result](evidence/m6/boot-128M/result.json) |
| QEMU 256 MiB | M1〜M6 PASS、exit 33、約48.64秒 | [serial](evidence/m6/boot-256M/serial-readable.log)、[result](evidence/m6/boot-256M/result.json) |
| 意図的panic | 診断一致、exit 35 | [result](evidence/m6/panic/result.json) |
| 想定外ページフォルト | 診断一致、exit 35 | [result](evidence/m6/unexpected-fault/result.json) |
| 2秒タイムアウト | kill・wait完了、exit -9 | [result](evidence/m6/timeout/result.json) |

正確なコマンド・cwd・終了コードは `evidence/m6/commands/*.json` にあります。
本測定は両RAM構成で計352件、ウォームアップ44件、合計396試行です。
各試行2つの実ユーザータスクを起動し、合計792タスクが正常終了しました。
M1〜M5の回帰、M4の4 MiB共有と違反検査、M5の18試行/起動も同じビルドで通過しています。
異常系ISOは意図的にM1/M2で停止するためM6を実行しません。

## 比較条件

各サイズ4／64ページ（16／256 KiB）についてdirtyページ数0、1、Nを試します。
ページiはバイト値i+1で初期化し、同一入力・同一ユーザーコード・同一VAを全方式で使用します。
両タスクが全バイトを読んでyieldし、writerだけが各dirtyページの先頭1バイトをXOR 0xff、
両タスクが全バイトを再読取りしてyield、最後にexitします。checksumはカーネルの計算値と一致し、
readerは常に元の内容を保持します。CPL3・異なる実CR3・全PTEの権限と物理一致も検査します。

| mode | 実装 | dirty負荷 |
| --- | --- | --- |
| eager-private | sourceを初期化・seal後、writer用に全ページを先行コピー。writer RW/NX、reader RO/NX | 0 / 1 / N |
| raw-shared-ro | fixture所有の同じ物理ページを両者にRO/NXで写像 | 0 |
| raw-cow | 初期RO共有。認可済みのwriter faultで当該ページを複製・置換 | 0 / 1 / N |
| object-shared-ro | 既存ObjectStore create/grant/mapのCapability検査を使用 | 0 |
| object-cow | 同storeのCOW権利検査・resolve_cowを使用 | 0 / 1 / N |

raw方式はカーネル内の有限試験が所有する対照群です。任意ユーザーへ生フレームや他タスクの
ページテーブル操作を公開しません。全方式でハードウェア隔離を維持します。
dirty=0は5方式、dirty=1/Nは3方式、2サイズで22条件です。各条件の最初の1回をウォームアップとして
保存し、続く8回を集計します。roundごとにモード順を回転し、外れ値を捨てません。
順序位置は完全な均等化ではなく、各RAM構成の正式結果は1起動分です。

## ピーク物理メモリ・コピー・fault

以下は両RAM構成の全サンプルで一致したpayloadピークです。1フレーム＝4,096バイトです。
copiesは初期入力の生成を除く追加の全ページコピー数、faultは実ユーザー書込み#PF数です。

| サイズ | dirty | eager-private frames / copies / faults | raw・object共有RO frames / copies / faults | raw・object COW frames / copies / faults |
| --- | ---: | ---: | ---: | ---: |
| 16 KiB | 0 | 8 / 4 / 0 | 4 / 0 / 0 | 4 / 0 / 0 |
| 16 KiB | 1 | 8 / 4 / 0 | 対象外 | 5 / 1 / 1 |
| 16 KiB | 4 | 8 / 4 / 0 | 対象外 | 8 / 4 / 4 |
| 256 KiB | 0 | 128 / 64 / 0 | 64 / 0 / 0 | 64 / 0 / 0 |
| 256 KiB | 1 | 128 / 64 / 0 | 対象外 | 65 / 1 / 1 |
| 256 KiB | 64 | 128 / 64 / 0 | 対象外 | 128 / 64 / 64 |

FrameAllocatorは全成功割当てでピークを更新し、解放で減らしません。各試行の開始時だけ現在のallocatedへ
リセットし、途中の一時割当ても含めます。ホストテストでは解放後のピーク保持、非ゼロbaselineへのリセット、
割当て失敗で増えないことを確認しました。試行後はallocated・表・写像が全てbaselineへ戻ります。

| 基礎会計 | 128 MiB | 256 MiB |
| --- | ---: | ---: |
| usable pool全フレーム | 29,950 | 62,718 |
| 試行開始前／終了後allocated | 89 / 89 | 153 / 153 |
| 開始前／終了後ページテーブル | 73 / 73 | 137 / 137 |
| 開始前／終了後仮想写像 | 31,371 / 31,371 | 64,139 / 64,139 |
| 各試行の追加ユーザーページテーブル | 14 | 14 |
| 各試行のタスク固有コード・スタック・秘密ページ | 5 | 5 |
| カーネル画像全体のメモリ範囲 | 786,432 bytes | 786,432 bytes |

プールピークは `baseline + 14 + 5 + payload_peak` です。256 KiBのprivate／全ページCOWでは
236／300フレーム、読取り共有では172／236フレーム、1ページCOWでは173／237フレームです。
64ページのpayload写像は2タスク計128仮想ページですが、物理payloadへ重複加算しません。

カーネル画像＋プールピークは、256 KiBのprivateが1,753,088／2,015,232バイト、
読取り共有が1,490,944／1,753,088バイト、1ページCOWが1,495,040／1,757,184バイトです。
この画像範囲はLimineのkernel_and_modulesと同じ786,432バイトで、コード・BSS・全store・allocator・
静的例外スタック等を含みます。M4/M5の未使用の試験用管理領域も常駐し、対象外として隠していません。
ブートローダ保持領域、ファームウェア、MMIO等を含むマシン全体のRAM消費量ではありません。

M6のraw管理配列1,024バイトとObjectStore管理配列24,384バイト、計25,408バイトは画像に常時含まれます。
方式ごとの管理量は内訳であり、方式切替で他方の配列が解放されるわけではありません。
ObjectStoreは64ページ容量の汎用固定表を持つため、2タスクだけに最適化した最小実装との比較ではありません。
2つのTask構造体400バイト等は既存カーネルスタック内です。別の物理確保として加算しません。
file-backedページと圧縮使用量は0です。M6にはM5のdedup/compressionを適用していません。

## 遅延とthroughput

全22条件の表は [128 MiB自動集計](evidence/m6/boot-128M/m6-benchmark.md) と
[256 MiB自動集計](evidence/m6/boot-256M/m6-benchmark.md) にあります。
全198生サンプルは各result.jsonの `m6.runs`、測定8件のmin/median/maxは `m6.summary` に保存しました。
時間は**QEMUゲストTSC tick**です。実CPUサイクル、校正済み秒、実機のメモリ帯域ではありません。
連続タイムスタンプ読取り差32回の最小値は41／39 tickで、測定値から差し引いていません。
成功経路のシリアル出力は計時区間外です。

以下は256 KiBで各8件の中央値です。小数は保持しています。

| RAM | dirty | mode | create tick | map tick | write/read tick | read bytes / 10^6 tick |
| --- | ---: | --- | ---: | ---: | ---: | ---: |
| 128 | 0 | eager-private | 3,057,388 | 161,744 | 1,731,161 | 274,686 |
| 128 | 0 | raw-shared-ro | 1,705,989.5 | 171,990 | 1,628,515 | 285,521 |
| 128 | 0 | object-shared-ro | 1,750,884.5 | 238,136 | 1,697,033 | 282,334 |
| 128 | 1 | eager-private | 2,886,826.5 | 194,134 | 1,645,532.5 | 279,482 |
| 128 | 1 | raw-cow | 1,417,317.5 | 227,546 | 1,778,131.5 | 272,151 |
| 128 | 1 | object-cow | 1,756,684 | 232,414.5 | 1,731,219.5 | 289,390 |
| 128 | 64 | eager-private | 2,811,336.5 | 168,242 | 1,720,992 | 276,090 |
| 128 | 64 | raw-cow | 1,501,420.5 | 237,434.5 | 4,214,772.5 | 263,628 |
| 128 | 64 | object-cow | 1,493,292 | 231,436.5 | 4,355,439 | 282,849 |
| 256 | 0 | eager-private | 3,061,030 | 162,569 | 1,596,706 | 275,584 |
| 256 | 0 | raw-shared-ro | 1,666,208.5 | 160,494 | 1,596,093.5 | 291,286 |
| 256 | 0 | object-shared-ro | 1,784,844.5 | 202,860.5 | 1,573,622 | 290,970 |
| 256 | 1 | eager-private | 2,808,590 | 157,228 | 1,627,663.5 | 291,971 |
| 256 | 1 | raw-cow | 1,390,565.5 | 188,351 | 1,712,856 | 293,710 |
| 256 | 1 | object-cow | 1,752,417 | 169,314 | 1,716,372 | 291,680 |
| 256 | 64 | eager-private | 2,611,600.5 | 181,826.5 | 1,660,434.5 | 283,568 |
| 256 | 64 | raw-cow | 1,402,605 | 195,279.5 | 4,154,571.5 | 288,466 |
| 256 | 64 | object-cow | 1,484,686 | 215,282.5 | 4,243,466.5 | 287,169 |

createには初期化・sealと、privateの全コピーまたはobjectのgrantが入ります。mapは2タスクへの
payload写像です。タスク生成は含みません。どちらもカーネルAPIのバッチ処理で、ユーザーsyscallの往復時間ではありません。
readは2タスクの全バイトchecksum読取りとyield・遷移を含みます。throughputは `2×N×4096 / read_ticks × 10^6`。
write/readにはdirty更新・fault・全バイト再読取り・遷移が含まれ、単独writeやメモリコピー帯域ではありません。

COW handler計時はコピーとPTE置換を含み、CPU trap入口・復帰は含みません。write/readに既に含まれる内訳です。
1ページdirtyでのhandler中央値はraw/objectが128 MiBで42,662／66,673.5、256 MiBで37,888.5／47,494.5 tickでした。
全64ページdirtyの試行内handler平均の中央値は128 MiBで26,557.34／29,442.08、
256 MiBで27,073.16／28,519.59 tickです。最大値も各試行と集計に保持します。p95等の推定はしません。

unmapは写像解除と私有コピー破棄、releaseは共有元のゼロ化・破棄を含みます。
例えば128 MiB・64ページdirtyのunmap中央値はprivate/raw-COW/object-COWが
1,207,674／1,195,285.5／963,818 tick、releaseは685,401.5／732,930／764,639.5 tickでした。
タスク終了とその固有ページ・rootの破棄、結果照合・診断出力はこれらの区間外です。

全ページ更新のCOWは64回のfaultとコピーを更新時に支払い、privateはコピーをcreateで先払いします。
更新区間だけの比率をアプリ全体の性能差としません。順序・JIT・ホスト負荷・キャッシュ状態の影響がある
TCGの8件の測定から、Capability方式の一般的な高速化／低速化率や実機性能は結論にしません。

## 検証器と適用範囲

検証器は22条件の正確な順序・198件・396タスク起動/bootを必須にし、実CR3/CPL3、checksum、
ピーク・管理量、fault/copy、全回収を照合します。合成ログの6追加テストでは欠落・並替え・重複、
ピーク過少申告、回収漏れ、内容破損、権限情報不整合、時間不整合、ウォームアップ混入を拒否します。
合成ログの成功をQEMUの起動証拠としては扱いません。起動証拠は上記の実シリアルとexitです。

未検証・未実装の範囲:

- 実機、KVM、校正済みwall time、SMP、プリエンプション、長時間運用、一般ユーザーELFの実行。
- 1／4／8タスクのスケーリング、多様な入力・ランダムな書込み位置、統計的有意差・tail latency。
- Linux等の実mmap／forkとの直接比較、file-backedページやストレージI/O。
- M5 adapterを素のBackendと比較する完全な管理・コードコスト測定、256 KiB以上へのM5ポリシー適用。
  M5のON/OFF回帰は通過していますが、専用32 backingの範囲です。
- 純粋なsyscall往復時間、全起動residentの網羅、unused storeを除去した最小ビルドとの比較。
- M6のOOM・置換失敗の実QEMU注入。既存ホストモデルの失敗・回収テストは継続して通過しています。

## 変更ファイルと次のM7

Git管理がないため、M5の保存済みハッシュと作業内容から変更を記録します。

- 新規: `src/m6.rs`、`src/m6_user.S`（5方式、実ユーザー負荷、計測・検査）。
- 更新: `src/frame.rs`（ピーク追跡・テスト）、`src/m4.rs`（M6起動）、`src/m5.rs`（計時関数の共有）。
- 新規: `tools/m6_validation.py`、`tools/test_m6.py`。更新: `tools/qemu.py`（検証・自動表・時間制限）。
- 更新: `README.md`、`docs/ARCHITECTURE.md`、`docs/ROADMAP.md`、`docs/BUILD.md`、`docs/DECISIONS.md`、
  M1〜M5検証報告の最新成果物への案内。
- 新規: 本報告と `docs/evidence/m6/`。[ソースハッシュ一覧](evidence/m6/source-hashes.json)を保存します。
  元ログは `artifacts/`、生成物は `build/` と `target/`。M1〜M5の証跡は変更していません。

次はM7です。言語隔離の単一アドレス空間、paging付き共通写像、Capabilityハードウェア方式を比較し、
対象コードと攻撃者能力・信頼境界を明記します。その後に限定的な試作、境界外アクセス・偽造・寿命違反の
攻撃テスト、M4/M6に対するメモリ会計を実装します。任意nativeコードの隔離をソフトウェアhandle検査だけで
成立させる設計にはしません。今回M7は未着手です。
