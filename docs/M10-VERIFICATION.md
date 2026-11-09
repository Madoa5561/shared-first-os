# Milestone 10 検証報告

M10は外部バイトコードの実行を、コードと権限ホストに束縛したSessionへ移しました。
途中returnやスコープ終了で残るtaskを自動停止し、当該taskのCapabilityと物理object参照を回収します。
最終ソースでの正式verifyの25コマンドが全て終了0で完了し、M10のゲートを通過しました。
設計と信頼境界は [M10-DESIGN.md](M10-DESIGN.md) に記載します。

## 実装と変更ファイル

- `src/session.rs`: safe RustのSession、同一hostの排他的borrow、codeのimmutable borrow、
  Dropによる残存taskのshutdown。空Batch/不正quantumはhost引受け前に拒否。
- `src/m9.rs`: 外部module実行をSessionへ移行。dispatchごとのhost指定と手動shutdownを除去。
- `src/m10.rs` / `src/m7.rs`: 実物理4 KiB objectを3taskで参照する4種類の終了fixture。
- `src/lib.rs` / `src/scheduler.rs`: Session公開とhost契約の明確化。
- `tools/m10_validation.py` / `tools/test_m10.py` / `tools/qemu.py`: 各caseのstop回数、
  frame/table/mapping収支、payload/cap/code回収を独立に照合。
- `tools/project.ps1` / `.github/workflows/check.yml`: Rustdocのcompile-fail/passを正式検証へ追加。
- README、ARCHITECTURE、ROADMAP、BUILD、DECISIONS、M10設計/本報告、証跡。

新しい依存、ホスト設定変更、物理ディスク操作はありません。既存nativeのCPL3/独立CR3を維持します。

## 検証手順と範囲

```powershell
./tools/project.ps1 verify
```

正式コマンド・出力・終了コードを [commands](evidence/m10/commands) に保存します。
Rust単体58件（M10追加4件）、Rustdoc 3件（借用違反のcompile-fail 2件、正常compile 1件）、
Python 51件（M10追加2件）、format、host/kernel Clippyが全て合格しました。
Rustdocは実行中のBatch.clearとhostへの別経路アクセスがコンパイル拒否され、
Sessionの破棄後には再borrowできることを確認します。実行時のメモリ回収は別途QEMUで検査します。

初回verifyも25コマンドが成功しましたが、その途中のhost契約コメント修正で最終ELFのhashが
起動済みISOと変わったため、最終ソースで全verifyをやり直しました。以下には最終実行を保存します。
初回ログはローカルの `artifacts/m10-verify.log` と `artifacts/commands-20261110-001110-402/` に保持します。

## 実QEMU

| 条件 | 判定 | 証跡 |
| --- | --- | --- |
| 通常128 MiB | M1〜M10 PASS、終了33、約71.05秒 | [result](evidence/m10/boot-128M/result.json) |
| 通常256 MiB | M1〜M10 PASS、終了33、約72.50秒 | [result](evidence/m10/boot-256M/result.json) |
| 外部codeの42→99 | 同一ELFで出力変更、M1〜M10 PASS | [result](evidence/m10/module-alt/result.json) |
| 不正版番号 | Version拒否、admitted=0、M10 PASS | [result](evidence/m10/module-bad-version/result.json) |
| 最後の不正opcode | Code(Opcode)拒否、admitted=0、M10 PASS | [result](evidence/m10/module-bad-opcode/result.json) |
| 2秒timeout | kill/reap、終了-9 | [result](evidence/m10/timeout/result.json) |
| 意図的panic | 診断・停止、終了35 | [result](evidence/m10/panic/result.json) |
| 想定外page fault | 診断・停止、終了35 | [result](evidence/m10/unexpected-fault/result.json) |

不正moduleではM9 PASSを出力せず、外部programを登録しません。M10はその後に別の信頼された
kernel fixtureを実行します。不正な外部codeを実行してから成功扱いにしているわけではありません。

各正常/module変更bootで次の4ケースを実行しました。

1. 3taskを登録し、dispatch前にSessionをDrop。
2. task 0がHALTし、残り2taskがある状態でDrop。
3. task 1をcancel（二度目はfalse）、task 0がHALT、残るtask 2をDropで停止。
4. task 0がAuthority fault、残る2taskをDropで停止。

全ケースで停止回数はtask別に `[1,1,1,0]`。payload/capabilitiesは0、Batch.clear後のcode/countも0。
allocatorピークは1frameだけ増え、128 MiBで89→89、256 MiBで153→153。
ページテーブルは73/137、仮想写像数31,394/64,162で前後一致しました。
M10は同一objectへの3つの参照とその寿命を検査し、CPUの同一ページ読取りは既存M4/M8の回帰で検査します。
M4の共有/COW、M5の18試行、M6の198試行、M7の22攻撃/36試行、M8の17dispatchも維持します。

## 成果物と会計

通常ISO、正常ELF、module、証跡コピーのSHA256を [artifact verification](evidence/m10/artifact-verification.json)、
ソースのhashを [source hashes](evidence/m10/source-hashes.json) に記録します。
verifyの最後に復元した正常ELFと起動ISO内ELFが一致し、module変更3種も同じELFであることを要求します。
ISOは日時メタデータを含むためbit単位の再現性を主張しません。

配布対象の通常ISOは `build/normal/shared-first.iso`、SHA256は
`97547ea195f2ede6e537fa31588f8bf33ff22d644da891b50dcb92380b9da5bd`。
正常ELFのSHA256は `a0293bf6a0300e15b0dfecbacd7a59ac851fafa729e2748f7fb744b08f31c999`。

Sessionは616 byte（Scheduler 608 byte + host参照8 byte）。Batchは1,096 byte、
固定stack上に置き、別の物理フレームとして二重加算しません。実stack高水位は未測定です。
カーネル画像は880,640 byte（M9より4,096 byte増）。元moduleの予約1ページ4,096 byteを保持し、
`kernel_and_modules`合計884,736 byteです。Drop/Batch.clearは元moduleページの回収を意味しません。
RAIIは既存のRustの手法であり、新しい隔離方式・RAM削減・速度向上を主張しません。

## 未検証・次段階

Dropは通常のreturn/`?`/スコープ終了で動作します。panic=abort、電源断、mem::forgetでは保証しません。
stopは信頼されたinfallible/no-reentry操作で、成功したSessionは登録済みslotだけを所有します。
外部codeはRust APIやホストの所有関係を操作できません。悪意あるkernel/hostは対象外です。
低水準Schedulerを直接使う場合には、同一host維持と明示的shutdownの責任が残ります。

外部codeの初期Capability付与、動的ロード、slot再利用、native preemption、SMP、実機、
元moduleの物理ページ回収、形式的な安全性証明は未実装/未検証です。
次は信頼されたlaunch manifestが明示的に付与する初期Capabilityと、外部program同士の
immutable object共有を実装します。外部code自身にtask IDや権利の増幅を許可しない境界を維持します。
