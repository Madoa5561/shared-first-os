# M14: 有界の起動・実行入口

runner::executeは信頼されたkernel用の同期API。外部byte列、専用ObjectStore、RunOptionsを受け取り、
設定検査→Batch全件検証→Launch準備→Session実行→残task取消→Launch回収→code clearを行う。
任意callbackやhost getter、自分を参照する構造体、heap、unsafeは追加しない。

quantumは1..64、dispatch_limitは0..1024。0は準備後に実行せず取消する正当な要求。
燃料quotaは従来通りで、dispatch上限は新しいCPU時間保証ではない。同期host呼出しは中断しない。
上限到達時にReadyが残る場合だけlimited=true。全停止なら上限と同数でもfalse。
実行中faultはタスクの終了状態で返し、起動失敗とは区別する。

ReportはResult<Completion,StartError>と実行前後のStatsを所有する。StartErrorは設定・Load・Launch・
Sessionの段階を区別する。Completionは最終Snapshot、dispatch数、limited、task stop回数。
成功でも失敗でも通常return前に取得した資源を回収する。busy拒否で既存資源は回収しない。
会計の一致はCapability世代の巻戻しを意味しない。code clearは論理的解放でsecure eraseではない。

Gate: 正常/正確な上限/途中取消/実行前取消、設定とcodeと件数拒否、busy store保持、
allocate/seal失敗のhost tests。実QEMUで外部moduleを新APIだけから実行し、同一frame、
終了理由/fuel、frame/table/mapping収支、全入力variant・M1〜M13回帰を照合。

READ manifest・module順序・呼出し元を信頼する。任意native実行、対話的制御、再開可能job管理、
SMP、native preemption、実時間上限、panic=abort/mem::forget時の回収は提供しない。
