# Milestone 11 検証報告

M11は信頼されたkernel内manifestから外部bytecodeへ初期READ Capabilityを渡します。
2programが同じimmutable 4 KiB objectを読み、無権限の3個目を拒否します。
180秒上限で正式verifyの31コマンドが全て終了0となり、M11のゲートを通過しました。
設計は [M11-DESIGN.md](M11-DESIGN.md) を参照。

## 変更ファイル

- `src/launch.rs`: 固定3program、空の専用store、READだけの委譲、初期引数、準備失敗/終了時の回収。
- `src/sandbox.rs` / `src/scheduler.rs` / `src/session.rs`: kernel由来のr1引数をtask別に受け渡す。
  既存new/admitは引数0を維持し、Context/Schedulerの固定サイズや命令形式は変更しない。
- `src/boot.rs` / `limine.conf`: 正確に2個のmoduleを取得し、index/size/retained領域を検査。
- `src/m11.rs` / `src/m7.rs` / `src/lib.rs`: 外部実行と物理backing/権限/寿命のQEMU検査。
  `src/m9.rs`はindex 0に固定し、従来の無権限試験を維持。
- `examples/bytecode/shared.json`: SUM/YIELD/HALTの3program。code 144 byte、fuel合計12,300。
- `tools/qemu.py` / `tools/project.ps1`: shared module生成、3追加variant、180秒上限、全31コマンド。
- `tools/m11_validation.py` / `tools/test_m11.py`: 入力照合、同一frame、認可、fuel、回収の独立検証。
- README、ARCHITECTURE、ROADMAP、BUILD、DECISIONS、形式/設計/検証文書、M11証跡。

新しい依存関係やホスト設定は追加しません。QEMU以外へ起動・インストールしていません。

## 実行コマンドと修正

```powershell
./tools/project.ps1 verify
```

最終記録は [commands](evidence/m11/commands)。Rust単体63件（追加5）、Rustdoc 3件、
Python54件（追加3）、fmt、host/kernel Clippyが合格しました。
host testsはtask別引数、READ限定/再委譲・map拒否、他taskのhandle、stale handle、範囲外、
busy store、件数不正、allocation/seal失敗、Session作成拒否と早期Dropを検査します。

開発中にテストのmap引数不足と不正HALT encodingを修正しました。正式checkで見つかった
Python共通parserへ渡すフィールド型の誤りも修正し、[失敗check](evidence/m11/development-check) を保存します。

初回verifyは正常128/256 MiBに成功後、module-altが90秒上限に達し失敗しました。
[失敗result](evidence/m11/timeout-90-failure/result.json) と [生ログ](evidence/m11/timeout-90-failure/serial.log)、
[初回commands](evidence/m11/initial-commands) を保持します。終了-9、passed=falseであり成功扱いにしません。
ログは既存M6ベンチマークの途中でpanicなしでした。直前の正常256 MiBも約84秒のため、
上限だけ180秒へ増やし全verifyを再実行しました。ホスト負荷や中断が原因と断定する測定はしていません。
検査項目・反復回数を省略せず、意図的なtimeout試験は2秒のままです。

## 実QEMUの最終結果（全件合格）

| 条件 | 検証した結果 | 証跡 |
| --- | --- | --- |
| 通常128 MiB | M1〜M11 PASS、終了33 | [result](evidence/m11/boot-128M/result.json) |
| 通常256 MiB | M1〜M11 PASS、終了33 | [result](evidence/m11/boot-256M/result.json) |
| module-alt | M9は99、M11の共有も成功 | [result](evidence/m11/module-alt/result.json) |
| module-bad-version | M9 Version拒否、M11成功 | [result](evidence/m11/module-bad-version/result.json) |
| module-bad-opcode | M9 Code(Opcode)拒否、M11成功 | [result](evidence/m11/module-bad-opcode/result.json) |
| shared-alt | M11の2readerが14,336を返す | [result](evidence/m11/shared-alt/result.json) |
| shared-bad-opcode | 最後のcodeを拒否、admitted=0/grants=0 | [result](evidence/m11/shared-bad-opcode/result.json) |
| shared-bad-count | 有効な2programをmanifestが拒否、admitted=0/grants=0 | [result](evidence/m11/shared-bad-count/result.json) |
| 意図的2秒timeout | kill/reap、終了-9 | [result](evidence/m11/timeout/result.json) |
| 意図的panic | 診断・終了35 | [result](evidence/m11/panic/result.json) |
| 想定外page fault | 診断・終了35 | [result](evidence/m11/unexpected-fault/result.json) |

最終実行の通常128/256 MiBは約48.21/49.16秒、module-altは約48.90秒でした。
初回より短い値ですが、ホスト条件を固定した性能比較ではなく、OS高速化の証拠とは解釈しません。

M11の通常入力は各taskがSUM(4096)/YIELD/HALT、初期fuel 4,100です。
task 0/1は同じ1frameを読み、28,672を返してfuel 2→1で終了します。
task 2は同じSUMでも初期権限がなくAuthority fault、fuelは3で停止。物理ページを読みません。
最終的にtask 0/1/2のstop各1回、owner参照1だけを残し、LaunchのDropで最後の参照を回収します。
shared-altではファイル中のSUM長だけ2,048へ変更し、同じELFで結果14,336、fuel 2,050→2,049になります。
単にIPCでデータをコピーした結果ではなく、実read backendのframe記録とreadonly PTEを検査しています。
M11はインタプリタ経由の読取りで、新たなnative CPL3プロセスの起動・写像ではありません。

## 会計と成果物

回収時のallocatorは128 MiBで89→89、256 MiBで153→153。ピークは各baseline+1です。
ページテーブル数73/137、仮想写像数31,397/64,165は開始/終了で一致。payload/capabilities/codeは0。
Launch 80 byte、Session 616 byte、所有Batch 1,096 byteをstackへ置きます。実stack高水位は未測定です。
カーネル画像892,928 byte（M10より12,288 byte増）、2moduleの元予約ページは8,192 byteを保持。
`kernel_and_modules`合計901,120 byteです。物理payload回収とboot moduleの予約保持は別会計です。

ISO/ELF/2moduleのSHA256、6variantが同じELFであること、保存コピーのhashを
[artifact verification](evidence/m11/artifact-verification.json) に記録します。
[source hashes](evidence/m11/source-hashes.json) はソース/設定/文書の照合用です。
配布対象の正常ELFはSHA256 `25c436c1467a5d50a31745a9700987db81c96989d860f2f6e34266f4e8b86f11`、
通常ISOは `cb8eec48bd0c36d0d8dca651bfb021d2ce4cd1dc7d16521d9c234baac8a057f4`。
共有用moduleは208 byte、SHA256 `c2d7eb57f97f0d189a0c24489dbb9fd828098fc85c07a901c013156d2f3064e2`。
ISOの日時は固定せずbit単位の再現性を主張しません。追加機能のRAM削減や速度優位性は主張しません。

## 未検証と次段階

manifestはコンパイル済みの固定設定で、module順序とbootloaderを信頼します。任意の検証済みcodeを
その入口へ配置すれば同じREADを得ます。外部codeの同一性・署名・出所を認証する仕組みではありません。
汎用外部manifest、動的task作成/slot再利用、native loader/preemption、SMP、実機、
module元ページの回収、形式的な安全性証明は未実装/未検証。panic=abort/mem::forgetではDropを保証しません。

次はSession内の信頼された制御操作として失効を追加し、外部programの中断・再開中の失効、
部分終了、再起動でのstale handleを実QEMUで検査する段階です。
