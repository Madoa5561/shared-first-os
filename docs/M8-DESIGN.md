# M8: 燃料制限付きバイトコードスケジューラ

## 選択理由と合格条件

M7では1つのContextがYIELD/HALT/燃料切れまでrunを占有する。外部入力や言語機能を
増やす前に、明示的YIELDをしないプログラムの実行を分割し、他タスクの進行を保証する。
M8は固定容量・有限batchに限定したスケジューラを実装する。

- 1回1〜64命令のquantumで中断、最大4タスクのround-robin。待ちは最大3回の他タスクdispatch。
- 全batchの累積燃料2,097,152、各タスクの燃料は0不可。終了・取消でも予算を払い戻さない。
- slotはbatch中再利用しない。拒否したadmissionでslot/予算を消費しない。
- hostがtask IDを固定し、命令にtask選択権を与えない。HALT/fault/cancelでhost.stopを1回呼ぶ。
- stopはCapability/写像を解放。生きたbatchの破棄前には明示的shutdownが必要。
- 検証済みimmutable Program、private scratch、全SUM時の認可を維持。
- RustテストとQEMUで公平性、暴走・失敗の局所化、容量・燃料上限、失効後の再開拒否、
  同一物理ページの読取り、全資源回収を検査。M1〜M7の回帰ゲートも維持する。

## 脅威モデル・限界

M7の攻撃者モデルを継承する。攻撃者は有効な任意命令列（YIELDなしloop/YIELD連打含む）を
選べるが、admission、quantum、host adapter、ObjectStoreにはアクセスできない。
スケジューラはsafe Rustの信頼されたCPL0コード。新しいbatch生成・予算付与は信頼された管理者のみ。
新batch前には全task表を空にし、Programの所有者を実行終了まで保持する。
同じSchedulerを複数のauthority hostへ付け替えることは管理者契約で禁止する。

公平性は命令単位。SUM最大256 KiBは同期処理で途中中断しない。1dispatchは最大
quantum×(262,144+1)燃料相当の命令処理。host.stop等の信頼コードとシリアル出力の時間は
この上限の外で、実時間保証ではない。nativeのタイマ割込み/SMP/外部ローダは未実装。
停止済みContextはbatch寿命中に保持し、固定メモリを再利用しない。Dropは外部storeを所有せず
cleanupを呼べないため、shutdownまたは全terminalを必須とする。プロセス一般の管理機構ではない。

## 会計

Scheduler/Contextは既存kernel stack上。size_ofで固定構造体の実バイト数を記録する。
Program bytes、ObjectStore、trace、payload frame、ページテーブルを別計上し、stack既予約領域や
カーネル画像へ二重加算しない。性能優位性や起動全体のRAM削減は今回の合格条件にしない。
