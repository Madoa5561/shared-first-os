# M13: 読み取り専用の実行状態・資源照会

Schedulerは固定4slotのTaskStateを保持する。Ready、Halted(value)、Faulted(reason)、Cancelledを区別し、
HALT/fault/cancel後もSessionが存在する間は残燃料と終了理由をコピーで取得できる。
YIELDとquantum満了はReadyのまま。重複cancelやshutdownで過去の終了理由を上書きしない。
slot再利用・燃料補充は追加しない。残燃料合計には停止taskの未使用分を含み、再配布可能量ではない。

Session::snapshotはSchedulerの固定長コピーを返す。InspectableHost限定のSession::inspectは
同じhostの集約資源数と失効状態も返す。Launchはobject数、payload frame数、capability数、
mapping page数、revokedのみを公開する。host参照、handle、物理アドレス、register、scratchは含めない。
Haltedの終了値は既存dispatchと同じ任意のu64であり、別途Capability表を公開するものではない。
照会APIは信頼されたkernel用でbytecodeへ公開しない。&selfのhost照会は副作用なし・非再入という契約。
単一CPUの同期操作間のsnapshotであり、SMPのatomic snapshotではない。

code検証とLaunch準備に失敗した場合は既存のResultを使い、Sessionを生成しない。
準備→実行→終了の所有権境界はBatch/Launch/Sessionの既存RAIIを維持する。
汎用managerを加えず、M13ではその前提となる有界の状態照会を完成させる。

Gate:
- host testsで全終了理由、YIELD/quantum満了、fuel不足、取消、未使用slot、拒否時の不変性を照合。
- 取得したsnapshotを書き換えても実行状態が変わらず、照会の有無でdispatch順・結果が一致。
- QEMUで外部module 2を1命令quantumで実行し、通常終了と取消+失効の全snapshotを独立検証。
  借用中のpayload/capability回収をAPIで観測し、実frame/read traceと終了収支も照合。
- 不正moduleは権限付与前に拒否。M1〜M12と失敗経路の全回帰を維持。

停止後のContext/codeメモリはscope終了まで保持する。照会で停止済み燃料やデータは消去しない。
物理RAMと固定管理メモリを別に報告する。実stack高水位、実機、SMP、native preemptionは対象外。
