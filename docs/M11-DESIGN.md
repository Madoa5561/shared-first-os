# M11: 信頼された起動設定と外部プログラムの共有読取り

M9の無権限moduleを維持し、2個目のLimine module `shared.sfbc` を追加する。
同じ有界コンテナを全件検証した後、kernelにコンパイルしたReadManifestだけが初期権限を決める。
これは署名付きの外部manifestや汎用アプリローダではない。bootloaderとmodule順序は信頼する。
当該位置の任意の検証済みcodeへ付与される権限であり、codeの同一性を認証する仕組みではない。

固定3programのtask 0/1に同じimmutable 4 KiB objectへのREADのみを付与し、task 2には付与しない。
所有者は非実行slot 3。空の専用ObjectStoreだけを引き受け、設定・件数拒否では既存資源を変更しない。
準備中の失敗やSession作成拒否でもLaunchのDropが取得済み資源を回収する。
Sessionは各実行taskを1回停止し、Launchは残存taskとownerを解放する。

初期引数r1に実際のtask-local slot+generation handleを渡す。数値自体は権限ではなく、SUMは常に
scheduler由来のtask IDでObjectStoreを照合する。コードは引数を書き換えられるが権限を増幅できない。
既存Session/new/admitは引数0を維持。新しい命令や書込みAPIは追加しない。

Gate: host testsでREAD制限、他task/古いhandle/範囲外読取り拒否、busy/count拒否、準備失敗の回収、
引数のtask別注入を検査。QEMUで外部2programの同一物理frame・checksum、無権限taskの拒否、
frame/table/mapping回収、同一ELFで読取り範囲変更、不正な最後のcodeの全件拒否を検査する。
M1〜M10回帰とpanic/fault/timeoutを維持し、ソース・バイナリhashと生ログを保存する。

単一CPU、同期host read、固定容量、panic=abort/mem::forget時のDrop非保証は継続。
2個の元boot moduleの予約ページは保持する。物理ハードウェア、SMP、動的権限manifest、署名、
native preemption、一般的な性能優位性や安全性の形式証明は対象外。
