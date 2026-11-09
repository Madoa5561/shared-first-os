# Milestone 7 検証報告

2026-11-09 JST。脅威モデルに限定した言語隔離の研究試作を実装し、M7のゲートを通過しました。
safe Rustの8命令インタプリタ、検査付きObjectStore読取り、燃料管理、22種類の攻撃・隔離テスト、
native／bytecodeの実QEMU比較を追加しました。nativeタスクの独立CR3・CPL3は維持しています。

**測定結果は同一ビルドの限定負荷で19フレーム（77,824バイト）のプール節約です。**
M7追加によりカーネル画像とブートローダ保持領域は増えたため、M6時点より起動全体の空きRAMが増えた
わけではありません。任意nativeコードや汎用WASMの安全な単一アドレス空間実行を実装したとも主張しません。

## 再実行・生成物

```powershell
./tools/project.ps1 verify
```

全体・内部18コマンドとも終了コード0です。QEMU自身のコードは正常33、期待したpanic/fault 35、
2秒タイムアウト-9です。正常版ELFを最後に復元し、両RAM構成で起動したISOをそのまま保持しています。

| 生成物 | パス | SHA256 |
| --- | --- | --- |
| UEFI ISO | `build/normal/shared-first.iso` | `1683a1f984b1ea203185f39eff6856bb54e6ed1e113ad15099e30ce20ba7e7a2` |
| Rust ELF | `target/x86_64-unknown-none/release/shared-first-kernel` | `f0ffeacec8924fcbfbb3160bbdf8004518eeffbc2285a6891b513d72736927a2` |

[生成物照合](evidence/m7/artifact-verification.json)に元証跡の場所とハッシュの一致を記録しました。
Rust 1.96.0、Limine v8.7.0固定コミット、QEMU 10.2.1、OVMF、q35 / qemu64 / TCG / 1 CPUです。
ネットワーク・ホスト物理ディスクを接続せず、依存関係追加・commit・push・branch作成は行っていません。

## 実際に通過した検証

| 検証 | 結果 | 証跡 |
| --- | --- | --- |
| Rust fmt | 成功 | [出力](evidence/m7/commands/02.log) |
| Rustホスト単体テスト | 43件成功 | [出力](evidence/m7/commands/03.log) |
| Clippy、警告をエラー化 | ホスト・全機能カーネル成功 | [ホスト](evidence/m7/commands/04.log)、[カーネル](evidence/m7/commands/05.log) |
| Python検証器 | 38件成功 | [出力](evidence/m7/commands/06.log) |
| release／ISO作成 | 成功 | [ビルド](evidence/m7/commands/07.log)、[ハッシュ](evidence/m7/image-normal/hashes.json) |
| QEMU 128 MiB | M1〜M7 PASS、exit 33、約67.58秒 | [serial](evidence/m7/boot-128M/serial-readable.log)、[result](evidence/m7/boot-128M/result.json) |
| QEMU 256 MiB | M1〜M7 PASS、exit 33、約68.87秒 | [serial](evidence/m7/boot-256M/serial-readable.log)、[result](evidence/m7/boot-256M/result.json) |
| 意図的panic | 診断一致・exit 35 | [result](evidence/m7/panic/result.json) |
| 想定外ページフォルト | 診断一致・exit 35 | [result](evidence/m7/unexpected-fault/result.json) |
| 2秒タイムアウト | kill・wait・exit -9 | [result](evidence/m7/timeout/result.json) |

正確なコマンド・cwd・終了コードは `evidence/m7/commands/*.json`、各出力は対応する `.log` です。
初回Clippyはunsafeブロック直前の安全条件コメント不足で停止しました。
[失敗記録](evidence/m7/development-check/05.log)を残し、説明位置を直した後の正式検証は成功しています。
QEMUで観測した起動失敗を隠して成功扱いしたケースはありません。

各正常起動で22攻撃ケースと36比較試行（4条件×warmup 1＋測定8）を実行しました。
両構成合計で44攻撃チェック、72比較試行、測定64件・warmup 8件です。
比較部で起動するnativeタスクは計72個、bytecode Contextも計72個です。攻撃用Contextは別です。
M4の4 MiB共有・COW・違反検査、M5の18試行/boot、M6の198試行/bootも同じビルドで成功しています。

## 隔離方式と信頼範囲

実装前に [脅威モデル・検証計画](M7-THREAT-MODEL.md) を記録し、paging付き共通VA、制限付き言語、
Capabilityハードウェアを比較しました。ハードウェア方式は今回のqemu64には実装していません。
M7のbytecodeはWASM互換ではなく、immutable objectのchecksumと64バイトの私有scratchを扱う小さい言語です。

攻撃者が選べるのは命令列・整数・分岐先です。16バイト固定命令、最大16命令、4registerに限定し、
HALT / CONST / SUM / LOCAL_LOAD / LOCAL_STORE / ADD / JUMP / YIELDだけを許可します。
検証済みProgramの表現はprivateで、Contextは同じimmutableな命令列を保持します。
localアクセスは境界検査し、SUMは最大256 KiBです。nativeポインタ、任意native命令、object書込み、I/Oはありません。

SUMは信頼されたHostが保持するtask IDで、既存ObjectStoreのslot/generation/READ権利と
checked offset+lengthを毎回検査します。physical frameをbytecodeから指定できません。
カーネルHardware Backendは保有中のseal済みROページだけを読みます。2つのContextの実読取りframe列を
traceして全要素が同じであることを確認しました。コピーした別データを共有と呼んでいません。

全命令に最低1、SUMに1+lengthの燃料を実行前に課します。燃料はyieldをまたいで保持し、HALT・エラー後は
再開を拒否します。revocation後の次のSUMは失敗します。既に読み取った整数を消す保証はありません。
1回のSUMは途中プリエンプションせず有限に処理するため、燃料は実時間応答の保証ではありません。

インタプリタは**信頼されたCPL0コード**で、kernel CR3を共有します。bytecodeのバイト列をCPU命令としては
実行しません。検証器・インタプリタ・ObjectStore・Backend・コンパイラ等にバグがないことが前提で、
runtime自身のバグに対する追加paging隔離はありません。nativeは従来どおり独立CR3・CPL3で動きます。
この違いをログにも `mode`、`cpl`、`root0/root1` として記録しています。

## 攻撃・拒否テスト

全22ケースは両RAM構成で成功しました。以下の名前はserialと `result.json.m7.attacks` に対応します。

| 分類 | ケース | 検査したこと |
| --- | --- | --- |
| 命令・形式 | unknown_opcode, native_opcode, object_write | 未定義命令・native命令のバイト・object書込み命令を受理しない |
| 検証器 | bad_register, reserved_bits, bad_jump, truncated, oversized | 範囲外register・予約ビット・不正jump・不完全／過大プログラムを拒否 |
| 私有メモリ | local_read, local_write, local_overflow | scratchの境界外読書き・最大整数offsetを拒否 |
| Object境界 | object_bounds, object_overflow | object末尾を越える範囲・offset加算overflowを拒否 |
| 認可 | foreign_handle, forged_generation, missing_read, malformed_handle | 他タスクhandle・不正世代・READなし・不正形式を拒否 |
| 予算 | loop_fuel, bulk_fuel | 無限jumpを停止、残量不足SUMはBackendへ到達しない |
| Context隔離 | scratch_isolation | 一方のlocal書込みが他方とimmutable objectを変えない |
| 寿命 | revoked_resume, stale_reuse | yield中の失効とslot再利用後の旧handleを拒否 |

失敗後のContext再実行はStoppedになります。拒否された読取りがBackendへ到達しないこともtrace件数で検査します。
sourceと再利用後のreplacementのchecksum、試験後の全回収を確認しました。
Host側の追加テストは全未知opcode、末尾のゼロ長読取り、ページ跨ぎ、権利・失効、燃料のyield保持、
scratch隔離、fallthrough停止等を検査します。Python側の6追加テストは不完全ログ、CPL/CR3の混同、
攻撃欠落、ピーク過少申告、管理量省略、checksum・燃料不整合、warmup混入、M6対照群不一致を拒否します。
これは攻撃指向テストであって、全入力の形式証明や網羅的fuzzではありません。

## 同一ビルド内のメモリ比較

入力は16／256 KiB、ページiの全バイトをi+1に初期化します。2タスクが全バイトを2回読み、checksum一致を検査します。
nativeはM6と同じObjectStore・機械語fixture・RO写像、bytecodeは同じstoreの検査付きSUMを使います。
4条件ごとにwarmup 1＋測定8回、native／bytecodeの順序をroundごとに交替します。

| RAM | データ | native pool peak | bytecode pool peak | native画像＋pool bytes | bytecode画像＋pool bytes |
| --- | --- | ---: | ---: | ---: | ---: |
| 128 MiB | 16 KiB | 112 | 93 | 1,298,432 | 1,220,608 |
| 128 MiB | 256 KiB | 172 | 153 | 1,544,192 | 1,466,368 |
| 256 MiB | 16 KiB | 176 | 157 | 1,560,576 | 1,482,752 |
| 256 MiB | 256 KiB | 236 | 217 | 1,806,336 | 1,728,512 |

1フレームは4,096バイトです。全サンプルで同じ値でした。payloadは両方式とも4／64フレームで変わりません。
差はnative専用のページテーブル14＋コード／stack／秘密ページ5＝19フレームです。
native側の秘密ページ1枚は今回の読取りでは未使用で、RXコード共有などの最適化も未実施です。
これはM6相当の対照群であり、nativeの不可避な最小コストが19フレームという意味ではありません。
同じ起動のM6 object-shared-roのプールピークとnative値が一致することを検証器も照合します。

開始前／終了後allocatedは128 MiBで89、256 MiBで153。ページ表は73／137、仮想写像は31,384／64,152です。
全試行後にこれらが一致し、object・Capability・payload・写像を回収しました。bytecode側に追加ユーザーPTEはありません。
nativeの追加仮想写像は2N＋5です。仮想サイズを物理消費へ重複加算していません。

画像範囲839,680バイトにはruntimeコード、M7 ObjectStore 24,384バイト、trace 520バイト、既存M4/M5/M6の
全store、allocator、静的スタックなどを含みます。方式を切り替えても画像内の不要な配列は解放されません。
VM Context 2個272バイトとbytecode 192バイト、trace比較コピー等は既存64 KiB kernel stack内です。
native Task 2個400バイトもそのstack内で、いずれも別の物理確保として加算しません。
Context 272バイトが実行中のstack総使用量ではなく、stackの実ピーク使用量自体は未測定です。
file-backedページ・圧縮使用量は0です。M5のdedup/compressionはM7比較に適用しません。

### M6チェックポイントとの違い

同一ビルド内の比較と、M6完成時のバイナリからの増分を区別します。

| 起動時の領域（128 MiB） | M6完成時 | M7完成時 | 増減 |
| --- | ---: | ---: | ---: |
| kernel_and_modules bytes | 786,432 | 839,680 | +53,248（13ページ） |
| bootloader_reclaimable bytes | 4,182,016 | 4,435,968 | +253,952（62ページ） |
| usable pool frames | 29,950 | 29,875 | -75 |

証跡は [M6 serial](evidence/m6/boot-128M/serial-readable.log) と [M7 serial](evidence/m7/boot-128M/serial-readable.log) です。
このkernelはbootloader-reclaimableをまだ解放しません。画像＋allocatorだけなら、M6完成時のnative比較に対し
M7 bytecodeは77,824−53,248＝24,576バイト少ない計算ですが、ブートローダ保持領域を含む空きpoolでは
75−19＝56フレーム少なくなります。起動全体の省メモリ化を達成したという結論にはしません。
保持領域増の観測と、その内訳の個別追跡は別であり、ブートローダ内部の詳細な割当て原因は未解析です。

## 時間計測

全結果は [128 MiB自動表](evidence/m7/boot-128M/m7-benchmark.md)、[256 MiB自動表](evidence/m7/boot-256M/m7-benchmark.md)、
各result.jsonの `m7.runs` / `m7.summary` にあります。下表は各8件の中央値、単位は**ゲストTSC tick**です。

| RAM | データ | 方式 | 初回read | 再read | 初回read bytes / 10^6 tick |
| --- | --- | --- | ---: | ---: | ---: |
| 128 | 16 KiB | native | 426,588.5 | 187,410.5 | 76,820 |
| 128 | 16 KiB | bytecode | 73,385.5 | 54,827 | 446,582 |
| 128 | 256 KiB | native | 2,122,253 | 1,887,709 | 247,071 |
| 128 | 256 KiB | bytecode | 877,266.5 | 840,437 | 598,569 |
| 256 | 16 KiB | native | 408,320.5 | 177,425 | 80,270 |
| 256 | 16 KiB | bytecode | 70,459 | 56,697 | 467,567 |
| 256 | 256 KiB | native | 1,967,295.5 | 1,787,279.5 | 266,505 |
| 256 | 256 KiB | bytecode | 940,560.5 | 894,898 | 557,500 |

計時は各2コンテキストの読取りからyieldまでです。nativeは実ring遷移とint 0x80、bytecodeはdispatch・
権限／境界検査・hostのページ読取り・trace記録を含みます。初期化・Program検証・終了・シリアル出力は区間外です。
bytecodeは2コンテキストを個別計時して合算し、途中のtraceコピー／照合を外しています。
nativeは2タスクを連続計時します。タイムスタンプの回数も同一ではありません。
throughputは `2×payload_bytes×10^6/read_ticks` で、実機のメモリ帯域や校正済み秒ではありません。

nativeは組込みbyte単位ループ、bytecodeのSUMは信頼されたRust host処理です。今回bytecodeの値が小さくても、
命令解釈一般がnativeより速いという比較ではありません。汎用プログラムの書込み・分岐・I/O能力も揃えていません。
各RAM構成1回の正式boot内の8サンプル、外れ値除外なしで、min/median/maxを保持します。
有意差、p95、実機性能、side channel耐性を結論にしません。

## 未検証・残作業

- 汎用WASM/nativeの単一アドレス空間実行、外部コードローダ、JIT、object書込み、任意host機能。
- 検証器の形式証明・網羅的fuzz、runtime自身を別プロセスで隔離する構成。
- SMP、プリエンプション、DMA、キャッシュ／投機等のside channel、実機・KVM。
- 外部攻撃者からの実ネットワーク入力。攻撃列は試験コードが構成して同じ検証器へ渡す方式です。
- Context数の拡張、quotaの継続的な再割当て、悪意あるnativeタスクの無限ループ停止。
- 最適化したnative対照群、一般アプリ・多数入力・書込み負荷、stack使用量、ブートローダ領域回収。
- Capabilityハードウェアの実装・測定、CPU hardwareによる同一CR3内のbytecode Context隔離。

## 変更ファイルと次の研究段階

Git管理はありません。M6保存ハッシュとの比較で変更範囲を確認します。

- 新規: `src/sandbox.rs`（検証器・実行系・テスト）、`src/m7.rs`（物理Backend・攻撃・比較）。
- 更新: `src/lib.rs`、`src/object.rs`（検査付きread APIとテスト）、`src/m4.rs`（M7呼出し）。
- 新規: `tools/m7_validation.py`、`tools/test_m7.py`。更新: `tools/qemu.py`（必須検証・自動表）。
- 新規: 本報告、`docs/M7-THREAT-MODEL.md`、`docs/evidence/m7/`。
- 更新: `README.md`、`docs/ARCHITECTURE.md`、`docs/ROADMAP.md`、`docs/BUILD.md`、`docs/DECISIONS.md`、
  M1〜M6報告の最新成果物への案内。元のM1〜M6証跡は変更していません。

[最終ソースSHA256一覧](evidence/m7/source-hashes.json)を保存します。生成物は `build/` と `target/`、元ログは `artifacts/` です。
定義済みのM1〜M7は完了しました。M8は未定義です。次の研究課題は、外部bytecodeの取込みとquota付き
スケジューラ、検証器のfuzz／モデル検査、runtime信頼基盤の縮小、最適化したnative対照群での再比較です。
今回の制限を外す機能追加には、先に脅威モデルと試験計画を拡張する必要があります。
