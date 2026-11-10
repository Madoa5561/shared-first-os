# M12: 中断中の外部program失効と再起動

Sessionに`RevocableHost`限定のrevoke操作を追加する。任意のhost参照や差替えは公開しない。
Launchは専用owner handleを保持し、信頼された制御要求でobject全体をrevokeする。
失効は参照/既存写像をObjectStoreで除去し、同じ呼出し内でpayloadを回収する。
Contextは停止せず、保存済みregister/PC/fuelを持って再開する。次のSUMは通常の世代付き認可で拒否。
すでに読み取った値を消去する保証はない。失効をbytecode自身から要求する命令は追加しない。

3個目のboot module `revocation.sfbc` は3programのSUM/YIELD/SUM/HALT。
全件検証してからM11と同じREAD manifestで起動する。既存2moduleは回帰用に維持。
M11同様、module順序とkernel設定を信頼し、codeの署名/同一性認証は行わない。

QEMU gate:
1. 2readerが同じframeを読みYIELD。Session経由で失効、再要求false。再開後両readerがAuthority拒否。
2. YIELD後にtask 0をcancelし、残るreaderを失効。cancelの再要求false、全taskのstop各1回。
3. 同じstoreで新Launch。旧handleをtask 0へ渡して拒否、新handleのtask 1は2回読取り成功。
   task 2へtask 1の数値を渡しても拒否。コード・物理frameの再使用と権限世代の再使用を混同しない。

全ケースでframe/table/mapping収支、payload/cap/code回収を照合する。失効後のread backend呼出しは0。
host unit tests、全回帰、最後のopcode不正を初期権限付与前に拒否する外部入力試験も行う。
単一CPU・同期host read・固定slotを維持し、SMP中のTLB shootdownやnative preemptionは実装しない。
一般的な再起動管理、古いデータの消去、panic=abort/mem::forget時のDrop、実機や形式証明は保証対象外。
