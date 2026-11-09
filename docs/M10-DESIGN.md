# M10: コードと権限ホストに束縛された実行セッション

M8の低水準Schedulerは毎dispatchでhostを渡すAPIで、稼働中に破棄する前のshutdownは
呼出し側の契約だった。M9で外部codeをロードするようになったため、初期Capability付与に先立ち
実行範囲と終了処理を構造化する。

`Session<'code,'host,H>`は検証済みBatchのProgram borrowと、同一TaskHostの排他的borrowを保持。
dispatch/cancelにhost引数はなく、外からhostを差し替えられない。SessionのDropは生きたtaskだけを
shutdownし、HALT/fault/cancel済みtaskへstopを再実行しない。通常return・`?`・スコープ終了にも適用する。
全Session終了後だけBatch.clearとhostへの別の排他的アクセスが可能。

空Batch/不正quantumは権限の引受け前にエラーとする。この時点のhost資源は呼出し側が引き続き所有する。
成功後はBatch内taskだけを管理し、対応するhost表は当該session専用とする。ほかのslot資源は管理しない。
低水準SchedulerはM8の回帰fixture用に残すが、M9外部実行はSessionへ移行する。

## 不変条件と限界

- 攻撃者は外部bytecodeを選べるが、Rust Session/Hostの所有関係へ介入できない。
- stopは信頼された、失敗せず再入しない実装。ObjectStoreのrelease_taskが実PTE/参照/物理寿命を処理。
- codeは検証済み所有Batchからのimmutable borrow。実行中の破棄/変更はコンパイル拒否。
- hostの排他的borrowはSession生存中続き、別経路でreleaseしたり別hostへ付け替えたりできない。
- panic=abort、電源断、明示的mem::forget等ではDropは保証しない。悪意あるkernel/host、SMP、
  形式証明、任意native codeの隔離を対象にしない。一般的なRust RAIIを利用し、新しい隔離方式とは呼ばない。

## Gate

Rust単体で開始前Drop、fault経由の早期return、cancel+通常終了、設定拒否を検査。
compile-fail 2件と正常compile 1件でcode/host借用境界を検査し、正式check/verifyとCIへ追加。
QEMUでは3taskに同じ実4 KiB objectのCapabilityを与え、開始前中止、1task正常終了後中止、
取消+途中中止、fault後中止の4ケースを実行。stop各1回、payload/cap/code回収、allocatorピーク+1、
開始/終了のframe/table/mapping収支を要求する。ここでの権限付与はkernel fixtureであり、
外部プログラムへのlaunch manifestは次段階。
