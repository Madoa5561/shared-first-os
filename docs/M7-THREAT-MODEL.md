# M7: 制限付き言語隔離の試作と検証計画

## 選択と比較

| 方式 | アクセスを強制する場所 | 今回の扱い |
| --- | --- | --- |
| 共通VA・個別paging | CPUのPTE、syscall時のCapability検査 | M4/M6を継続。native比較群 |
| 単一アドレス空間・制限付き言語 | 命令検証器、インタプリタ、検査付きhost API | M7で小さな独自バイトコードを試作 |
| Capabilityハードウェア | タグ付き権限・境界等を扱うCPU | 今回のqemu64には導入しない。未実装・未測定 |

[WebAssemblyのsecurity model](https://webassembly.org/docs/security/) はホストとモジュールの隔離を
runtimeの責任とします。ここではその一般的な制限実行の考え方を参考にしますが、WASM互換性や
既存WASM実装の安全性を継承するものではありません。
[CHERI architectural rules](https://ctsrd-cheri.github.io/cheri-c-programming/background/architectural-rules.html)
はCapabilityの偽造防止や権限・境界の単調性をハードウェアで強制します。本試作の整数handleは
その代用ではなく、既存のtask-local tableとgenerationによるソフトウェア認可です。

## 脅威モデル

攻撃者は命令列の全バイト、整数定数、分岐先、実行順を選び、他者のhandle値を知っていると仮定します。
攻撃者はruntimeのtask ID、Capability表、Rust参照、host APIの実装を変更できません。
権限は信頼されたホストがobjectを作成・grantして与えます。ある数値が他タスクの有効handleでも、
自タスクで発行されていなければアクセスできません。同じ数値が自タスクで有効なら自タスクのentryを指します。

信頼基盤はRustコンパイラ、検証器、インタプリタ、ObjectStore、物理Backend、既存カーネル、ブート環境です。
インタプリタはカーネルCR3・ring 0で動く信頼されたnativeコードです。攻撃者のバイト列はデータとして
解釈するだけで、native命令として実行しません。runtimeのバグに対する別のハードウェア隔離はありません。
nativeユーザータスクには引き続き独立CR3・CPL3を使い、pagingを外しません。

保証対象は制限言語からのobject境界外読取り、他タスクの私有領域アクセス、handle偽造・世代再利用、
失効後の新しい読取り、無制限ループの拒否です。認可済みの読取り結果を既に得た後、その値を忘れさせる
保証はありません。時間・キャッシュ等のside channel、DMA、悪意あるkernel、SMP、実時間応答は対象外です。

## 命令と不変条件

命令は16バイト固定、最大16命令。先頭4バイトはopcode/dst/src/aux、次の8バイトはLE u64即値、
最後4バイトはゼロ予約です。レジスタは4個、各コンテキストの私有scratchは64バイト。
HALT、CONST、SUM、LOCAL_LOAD、LOCAL_STORE、ADD、JUMP、YIELDだけを許可します。
SUMはhandleレジスタ、offsetレジスタ、即値lengthを使い、immutable objectのchecksumだけを返します。
nativeポインタ、object書込み、I/O、syscall、間接native呼出し、JIT、動的メモリ確保はありません。

1. 検証器は到達不能な命令も含め全命令・予約ビット・レジスタ・分岐先・SUM上限256 KiBを検査します。
   Programはprivateな検証済み表現で、命令列をimmutable borrowします。実行中のコード書換えは許しません。
2. Contextは生成時から同じProgramを保持し、yield後の別program差替えを許しません。
   PC・register・scratch・燃料残量はprivateです。数値はポインタに変換しません。
3. localアクセスは毎回64バイト境界を検査します。加算は定義済みのwrapping整数演算です。
4. SUMは毎回task-local handle/generation/READ権利とchecked offset+lengthを検査します。
   Backendへ渡すのはObjectStoreが保持する実backingとページ内範囲だけです。Rust参照をbytecodeへ返しません。
5. 全命令は最低1燃料、SUMは1+length燃料を実行前に消費します。残量はyieldをまたいで保持し、
   エラー・HALT後は再開しません。失効は次のSUMで認可を再検査します。単一CPUの直列処理に限定します。

## 検証計画と合格条件

- Rustホストテスト: 不正形式・opcode・register・予約ビット・jump、scratch境界、算術、燃料、
  yield状態、失敗後の再実行拒否、objectの末尾・overflow・権限・世代・失効を検査。
- 実QEMU: 物理objectに対する不正handle、他タスクhandle、権利不足、範囲外、失効中断・再利用、
  不正命令・native escape試行、私有scratch隔離、無限loopとbulk処理の予算超過を検査。
- 比較: 16／256 KiB、各方式warmup 1＋測定8。nativeとbytecodeの2コンテキストが同じpayloadを
  2回読む。全checksum・物理backing一致、実CR3、ページ表／task frame数、ピーク、完全回収を必須にする。
- nativeはM6と同じfixture・ObjectStoreを使用。bytecode側のbulk SUMは信頼されたhost処理なので、
  時間差を一般言語や任意nativeアプリの性能差としない。物理ページ削減と制限の代償を併記する。
  native側の固有5フレームには今回読まない秘密ページ1枚も含む。コード共有等でさらに縮小できるため、
  この対照群をnative方式の最小コストとは扱わない。
- カーネル画像にruntime・全store・検証用traceを含め、Context・bytecodeのスタック内使用量を別記する。
  同一バイナリ比較であること、最小ビルドやマシン全体の消費量ではないことを明記する。

これはテストによる研究用の保証範囲です。形式証明・汎用言語ランタイムの完成を主張しません。
