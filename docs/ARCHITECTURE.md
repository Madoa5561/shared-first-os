# Architecture: Object-Centric Memory (OCM)

## M10の実行セッション（現在）

`src/session.rs` のSessionはSchedulerと、同じTaskHostの排他的borrowを保持する。
Programは所有Batchをimmutable borrowし、Session生存中のBatch.clearと別経路のhost操作をRustが拒否する。
dispatch/cancelは外からhostを受け取らない。Dropが残るtaskをshutdownし、既に終了したtaskのstopを重ねない。
M9の外部実行をSession経由へ移し、途中return時の明示的shutdownへの依存を除いた。

空Batch/不正quantumは引受け前に拒否し、host資源は呼出し側が保持する。成功後に管理するのは
Batchのslotだけで、他taskの資源まで巻き込まない。stopは信頼されたinfallible/no-reentry操作。
panic=abort、電源断、mem::forgetでの自動回収は保証しない。M8低水準Schedulerの直接利用には旧契約が残る。
compile-fail/compile-passで借用境界、QEMUで実payloadの中止/途中終了/fault後の回収を検査する。
詳細は[M10-DESIGN.md](M10-DESIGN.md)。外部codeへの初期権限付与はまだ実装しない。

## M9の外部コードロード

`src/boot.rs` がLimine moduleのABI prefixを読み、件数1・非NULL・最大592 byte・kind 6内の
範囲を検査する。M2のHHDMに保持した領域から読み、元の予約ページは回収しない。
ブートローダのメタデータ/ポインタは信頼するが、ファイル内容は信頼しない。

`src/loader.rs` はsafe Rustのversion付きコンテナparser。件数、entry長、reserved、命令、
fuel/code合計quota、切断・末尾余剰を全件検査し、成功時だけ所有Batchを返す。
Batchは最大4×256 byteのコード領域を持ち、実code上限は512 byte。Programはこのコピーをborrowする。
部分成功を外へ出さない。元のバイト列を後から変更してもコピーには反映しない。

`src/m9.rs` が空のCapability表を前提にSession経由でadmitする。初期権限の付与は実装しておらず、
外部コードの整数handleは権限にならない。認可は既存ObjectStoreへ接続し、終了時にrelease_taskする。
全task停止後にborrowを終え、Batch.clearでcode/count/fuelを0にする。実行中のclearは借用規則で禁止する。
実stack高水位やsecure erase、元moduleページの回収を保証しない。

ビルドは`examples/bytecode/demo.json`から`tools/pack_bytecode.py`で独立したprograms.sfbcを生成しISOへ格納。
同一ELFで42/99の別moduleと、不正version/最後のopcodeの拒否を起動試験する。
診断FNV-1aは認証ではなく、host側SHA256と合わせた入力照合。制約は [M9-FORMAT.md](M9-FORMAT.md)。

## M8の燃料付きスケジューラ

`src/scheduler.rs` はsafe Rustの固定4slot round-robinです。`Context::run_slice` が
1〜64命令ごとに中断し、YIELDのないプログラムからも制御を取り戻します。命令列とscratch、PC、
register、残燃料は同じContextに保存されます。生涯燃料とは別の命令数quantumです。
batchの累積付与量は2,097,152以下。終了・取消で返却せず、slotも再利用しません。
不正admissionは状態不変です。ホストだけが新batchを生成でき、当該batch専用のtask表だけを扱います。
M8/M9は空の表から開始し、M10の回収fixtureは信頼されたkernelが用意した権限を引き受けます。

`TaskHost` adapterへ渡すtask IDはschedulerのslotから決定します。bytecodeには選択権がありません。
HALT/fault/cancelでstopを1回だけ呼び、実adapterは既存`release_task`でCapability・写像を解放します。
shutdownは生きたtask全部に同じ処理を行います。スケジューラはstoreを所有しないため、
稼働中に捨てる前のshutdownと同一hostへの固定は信頼された呼出し元の契約です。
停止Contextの固定メモリはbatchが破棄されるまで保持し、燃料上限やslot上限を潜り抜ける再利用はありません。

`src/m8.rs` はM7の物理storeを再利用し、暴走loop・正常終了・YIELD連打・境界faultを混在実行します。
実物理1ページを2Contextで読み、SUM直後の強制中断中にrevokeし、再開後に認可を拒否します。
不許可のtaskによる読取りも生きたobjectに対して拒否。取消で共有先の寿命を保ち、最後の終了で回収します。
全17dispatchの順序・結果・燃料をシリアルに記録し、Python検証器で独立照合します。

単一CPU・割込み無効を維持します。公平性は最大3回の他タスクdispatch待ちという命令上の性質です。
SUM最大256 KiBとhost.stop等は途中中断しません。native taskのpreemption、実時間応答、SMP、
外部ローダは未実装です。[M8-DESIGN.md](M8-DESIGN.md) に前提・資源会計を記載します。

## M7の制限付き言語隔離

実装前に [M7-THREAT-MODEL.md](M7-THREAT-MODEL.md) で脅威モデル・方式比較・検証計画を定義しました。
`src/sandbox.rs` はsafe Rustだけの独自インタプリタです。WASM互換や任意nativeコードの実行はありません。
命令列はimmutable borrowで保持し、検証後の書換え・yield後のprogram差替えを防ぎます。
全命令を検証し、未知opcode・範囲外register・予約ビット・不正jumpを拒否します。
16バイト命令最大16個、4整数register、コンテキスト内scratch 64バイトに制限します。

命令はHALT、CONST、SUM、LOCAL_LOAD、LOCAL_STORE、ADD、JUMP、YIELDの8個です。
localアクセスは毎回境界検査し、ADDはwrapping整数加算です。nativeポインタ・I/O・object書込みはありません。
SUMは最大256 KiBのimmutable範囲を読み、checksumのみ返します。任意のデータ処理言語ではなく、
境界検査・寿命・資源制限を検証する小さい言語です。汎用WASMやアプリローダの代替ではありません。

ObjectStoreの `read_sum` が毎回task-local handle/generation/READとchecked offset+lengthを照合します。
RuntimeのHostがtask IDを保持し、命令列にはtask IDを指定する引数を与えません。
ReadBackendには保有中のseal済みframeとページ内範囲だけを渡します。HardwareはROのHHDMを確認し、
volatile読取りを行います。bytecodeに物理アドレス・Rust参照は返しません。native COW viewの内容を読むAPIでもありません。
失効・releaseは次のSUMの認可を拒否します。既に得た整数結果を消す保証はありません。

燃料は全命令で最低1、SUMで1+lengthを呼出し前に差し引き、yieldをまたいで保持します。
不足・例外・HALTでContextは停止し、再開を拒否します。SUMの1操作は有限ですが途中プリエンプションはなく、
実時間の応答上限を保証しません。新しいContextの生成や予算付与は信頼されたホストだけが行います。
nativeの既存協調タスクに、このbytecode燃料制御が適用されるわけではありません。

### 特権と信頼基盤

インタプリタ自体はカーネルCR3・CPL0で動く信頼されたコードです。bytecodeはCPU命令としては実行されません。
この制限言語に対するソフトウェア隔離であり、任意nativeコードをCPL0へ移したものではありません。
検証器・インタプリタ・ObjectStore・Backend・Rustコンパイラ等が信頼基盤に入り、そのバグに対する
二重のpaging隔離はありません。M4/M6のnativeタスクは独立CR3・CPL3を引き続き使用します。
SMP、DMA、side channel、悪意あるkernel、形式証明は対象外です。

### 比較・会計・証跡

`src/m7.rs` は22種類の拒否／隔離試験と、16／256 KiBのnative／bytecode比較を行います。
2コンテキストが同じ内容を2回読み、各方式warmup 1回＋測定8回を保存します。
nativeはM6と同じring-3 fixture・ObjectStoreで、同じVA・同じ物理ページのRO写像を検査します。
bytecodeはBackendが実際に読んだ全物理frameをtraceし、2コンテキストで完全一致することを検査します。
異なるnative CR3・CPL3と、同じkernel CR3・CPL0のbytecodeを別々に記録します。

M7 ObjectStore 24,384バイト、検証trace 520バイトは静的で全方式共通、画像に含みます。
2つのVM Contextは272バイト、命令列192バイトを既存kernel stackに保持します。
traceの比較用コピーや一時変数も同じ既存スタック内で、272バイトがスタック全使用量という意味ではありません。
native Task構造体は2個400バイト、固有物理ページは5、追加表は14。5ページには未使用の秘密ページ1枚が
含まれ、共有RXコードなどでさらに削減できるため、native方式の理論的な最小コストとは比較していません。
カーネル画像＋実allocatorピークを報告し、既存M4/M5/M6の管理配列も含めます。

read/repeatのゲストtickは2コンテキストの読取りとyieldまでを測り、初期化・検証・終了処理・シリアル出力は
除きます。bytecode側は2回の個別計時を合算し、その間のtraceコピー・照合は外します。
native側は2タスクを連続して計時します。SUMのhost処理・検査・trace記録はbytecode側の計時に含みます。
同じ読取り結果でも実行機構が異なり、時間比を一般アプリの高速化やCPUサイクル数とは解釈しません。
実測・限界・修正履歴は [M7-VERIFICATION.md](M7-VERIFICATION.md) を参照してください。

## M6のベースライン比較

`src/m6.rs` と `src/m6_user.S` はM4の実ユーザー遷移・ページテーブルを使う有限ベンチマークです。
16／256 KiB、dirtyページ数0／1／全ページを比較します。読取りのみは5方式、更新時は
privateと2種類のCOWの3方式、合計22条件です。各条件1回のウォームアップと8回の本測定を実行し、
モード順をroundごとに回転します。順序位置の完全な均等化や統計的有意差は保証しません。

| 方式 | writerの初期状態 | 権限・コピー |
| --- | --- | --- |
| eager-private | 全ページを先に複製、RW/NX | kernel fixtureの所有ページ、追加コピーN回 |
| raw-shared-ro | readerと同じ物理ページ、RO/NX | fixtureが直接写像、更新負荷には使用しない |
| raw-cow | readerと同じ物理ページ、RO/NX | fixtureの認可範囲だけfaultで1ページ複製 |
| object-shared-ro | readerと同じ物理ページ、RO/NX | 既存ObjectStoreのCapabilityを照合 |
| object-cow | readerと同じ物理ページ、RO/NX | CapabilityのCOW権利を照合して複製 |

raw方式は信頼されたカーネル試験の対照群であり、生フレームへの新しいユーザーAPIではありません。
全方式で独立CR3・CPL3・コードRX・データNXを維持します。共有元とHHDM別名はseal後ROです。
同じユーザーコードが全バイトを読み、writerはdirtyページ先頭の1バイトだけを反転し、両者が再読取りします。
checksumと全PTEの物理一致／私有化・権限を検査し、reader不変と試験ごとの完全回収を必須とします。
通常共有／COWもobject方式と同じpayload削減になる比較であり、汎用OSのmmap実装との直接比較ではありません。
M5ポリシーはM6比較では無効で、その専用storeのA/B回帰試験を別途継続します。

### ピークと管理メモリ

FrameAllocatorは成功した割当てごとにピークを更新し、解放で減らしません。各試験の開始時に
現在のallocated値へresetします。最終収支だけで一時的な割当てを見逃さない方式です。
ピークは開始前プール使用量＋追加ユーザー表14フレーム＋タスク固有5フレーム＋payloadです。
payloadはprivateが2N、共有がN、COWがN＋dirtyです。仮想payload写像2Nは別の論理量です。

raw管理配列は1,024バイト、64ページ容量のObjectStoreは24,384バイトで、両方を同じバイナリに
常時予約します。方式ごとの値は参考内訳で、使わない方式の領域が解放されるという意味ではありません。
リンカのkernel start/endの差には全コード・BSS・M4/M5/M6管理配列・allocator・静的例外スタック等を含みます。
`kernel_image_plus_pool_peak_bytes` はこの画像サイズ＋実プールピーク×4,096です。管理配列をさらに加算しません。
2つのTask構造体400バイト等の一時変数は既存カーネルスタック内です。file-backedページと圧縮使用量は0です。
ブートローダ・ファームウェア・MMIO等はこの管理下resident会計の対象外で、マシン全体のRAM使用量ではありません。

### 時間計測

M5と同じフェンス付きRDTSCを使用し、成功経路の計時区間にはシリアル出力を入れません。
createは入力生成・初期化・sealと、privateの全コピーまたはobjectのgrantを含みます。
mapは2タスクのpayload写像、readは2タスクの全バイトchecksum読取り・遷移・yieldを含みます。
write_readは更新と同じ全読取り、fault、遷移を含む複合区間です。単独writeの遅延ではありません。
COW handler時間はコピー・PTE置換（objectでは権限照合も含む）で、CPU trap入口／復帰を含みません。
handler時間はwrite_readの内数です。unmapは私有コピーの破棄、releaseは元payloadの破棄を含みます。
タスク生成・終了と固有ページ／root破棄、結果照合・シリアル表示はこれらの区間外です。
map/createはカーネルAPIを直接呼ぶ計測であり、ユーザーsyscall単体の往復遅延ではありません。

各試験の8種類の時間、handler最大値、min/median/max、read bytes / 1,000,000 guest ticksを保存します。
32回の連続タイムスタンプ差の最小値も記録し、測定値から差し引きません。ゲストtickを実CPUサイクルや
秒へ換算しません。ウォームアップを除く全サンプルを保持し、外れ値による選別や性能閾値による合否判定はしません。
再現手順と実測結果は [M6-VERIFICATION.md](M6-VERIFICATION.md) を参照してください。

## M5の任意ポリシー

`src/policy.rs` は既存ObjectStoreと物理Backendの間に置く、依存クレート・ヒープ・unsafeを使わない
固定容量のポリシーです。`src/m5.rs` が専用storeで明示的に有効／無効を切り替えます。
M4の4 MiB storeは従来のBackendのままです。ポリシーを全タスクへ暗黙に適用しません。
既知の重複排除とランレングス圧縮を組み合わせた実験であり、新規発明や既存OSに対する優位性ではありません。

### Backing IDと独立した権限

通常のHardware Backendは物理フレームを返しますが、Policy Backendはカーネル内部のbacking tokenを返します。
tokenはユーザーに公開せず、世代付きCapabilityとobject identityの検査は従来どおりObjectStoreが行います。
内部tokenの再利用は全参照・写像の消滅後だけです。map / unmap / COW置換ではtokenと実物理フレームの対応を
ポリシーが追跡し、Hardwareには実フレームだけを渡します。通常Backendの所有権は1参照、Policyの所有権は
参照付きbackingであり、destroyはその参照を解放し、最後の参照でのみ実フレームを破棄します。

immutableページのfingerprintを候補絞込みに使い、**全4,096バイトの比較が一致した場合だけ**集約します。
ハッシュは認証ではありません。末尾をゼロ化したページ全体で比較し、未sealページ・私有COWページは候補にしません。
別々にcreateしたオブジェクトの権限・参照・revoke範囲は混ぜず、内容だけを共有します。
一方をrevokeしても、別オブジェクトの生存参照と写像は保持します。u32参照数は上限を検査します。
8回連続して一致しなければ、その試験epochではハッシュ計算・候補探索を停止します。
空のstoreに対する再configureだけがepochを再開します。周期的な再学習や自動チューニングはありません。

重複による遅延・メモリ量の差は同じポリシードメイン内で観測可能です。M5は明示的に同意した試験store内に
限定し、互いに秘密を持つ任意タスク間の透過的な重複排除にはしません。タイミング秘匿は未実装です。

### 圧力時圧縮と再map

カーネルがmaintenanceを呼び、実free framesが指定watermark以下のときだけ圧縮を試みます。
対象はseal済み、未写像、非私有、residentのページです。写像数は実PTEの作成・除去・COW置換と連動します。
対象にPTEがないため、維持中のユーザー写像を除去したりTLBの安全条件を弱めたりしません。

RLEはrun長u16と値u8の3バイト形式です。各backingに予約した128バイト以下で全ページを表現できた場合だけ
採用し、コピーを保持してから元の実フレームをゼロ化・解放します。表現が収まらなければresidentのままです。
失敗したimmutableページは内容が変わらないため再試行しません。圧縮できる一般データでも128バイトを超えれば拒否する
保守的な方式で、汎用圧縮ライブラリではありません。ゼロページに限らず異なるrunや任意バイト値を復元できます。

再map時には全4 KiBを復号して新規フレームを初期化・sealし、その後でRO/NX PTEを作ります。
確保またはseal失敗では圧縮表現を保持し、途中の新規フレームを回収します。
PTE作成失敗では写像を残さず、復元済みresidentを有効なbackingとして保持します。
圧縮ページへのCPU faultで遅延復元する機構はなく、map syscallの時点で復元します。

COWはM4と同じ権限照合・1ページコピーです。私有コピーは常に非集約・非圧縮であり、
元backingを破壊しません。release / revoke / exitで当該写像を先に除き、参照が尽きたresidentまたは
圧縮backingを回収します。SMP・並行maintenanceは未対応で、単一CPU・IF=0・ユーザー停止中に直列処理します。

### 容量、会計、測定範囲

ポリシー容量は32個のdistinct backing（私有コピーを含む）、64個のページ写像です。
M5のObjectStoreは4ページ/オブジェクト、8オブジェクト、4タスク表、各16 Capability、32 viewです。
容量を超える要求はエラーにします。M4の4 MiB機能をこの小さいポリシーで置き換えたわけではありません。

固定メモリはobject metadata 5,184バイト、policy metadata 3,448バイト、圧縮予約領域4,096バイト、
合計12,728バイトです。圧縮領域は32×128バイトで各descriptor内に分散し、独立した物理1フレームではありません。
カーネル画像の予約領域に含まれ、圧縮使用量が3バイトになっても予約領域の実占有は消えません。
compressed bytesは使用中の符号列の合計で、予約領域に含まれるためもう一度加算しません。
ObjectStoreのpayload_framesはBackendに実resident数を問い合わせ、論理参照数を物理フレーム数と混同しません。
独立したfile-backedページはまだ0です。

A/Bの各側は4オブジェクト×16 KiB、同一入力hash・同一処理を使います。負荷は全ページ同一、
各ページが異なる一定値、決定的疑似乱数の3種類です。各側3回、順序はOFF→ON、ON→OFF、OFF→ONです。
圧力なしのmaintenance、しきい値をfree_before+32にしたmaintenance、2実タスクでのmap/read、
COW、別objectのrevoke後の読取り、全回収を実行します。これは制御した高いしきい値であり自然なOOMではありません。
各タスクの実CR3・CPL3とchecksum、PTEの権限・物理一致を検査します。

2タスクで8ユーザーページを写像し、固有ページ5フレームと追加ページテーブル14フレームを別計上します。
未写像時の部分会計はresident×4,096+12,728バイトです。カーネル全体、コード増分、ブートローダ等を含む
OS全体のRAM使用量ではありません。有効／無効で同じ予約領域を持ちます。M6ではポリシーなしの
private／共有／COWを比較します。M5 adapter自体の素のBackendに対する全コスト比較は未実施です。
タイミングはフェンス付きRDTSCで得るQEMUゲストtickです。createは入力生成を含み、pressureは2回のmaintenanceを含みます。
map/read・COW/read・cleanupはユーザー実行とシリアル診断も含むため、単独syscallの遅延ではありません。
測定のmin/median/maxと制限は [M5-VERIFICATION.md](M5-VERIFICATION.md) に記録しています。

## M4の特権境界（M5でも継続する基盤）

`src/m4.rs` がM2の物理メモリ・ページテーブルとM3由来のオブジェクト管理を使い、
実ring-3タスクを協調実行します。`src/user_cpu.rs` / `src/user_entry.S` が特権境界、
`src/user_program.S` が実際にCPL3で動く組込み試験コードです。
単一CPU・IF=0・PCID/PGEなしに限定します。無限ループするユーザーを強制切替できず、
試験全体の停止はホスト側QEMUタイムアウトに依存します。一般用途のプロセス環境ではありません。

### 特権境界とアドレス空間

独自GDT/TSSとIDTをロードし、IRETQでユーザーCS=0x23 / SS=0x1bに移行します。
int 0x80のゲートだけDPL3とし、例外とその他のゲートはDPL0です。TSSのRSP0と
double-fault用IST1に各16 KiBの静的スタックを用意します。これらはカーネル画像に含まれ、
専用ガードはありません。実際のスタック枯渇やdouble faultからの安全な処理は未検証です。
ユーザーRFLAGSはIF/IOPL/NT/VM等を除去し、TSSのI/O bitmap境界で直接I/Oを拒否します。
FPU/SIMDコンテキスト保存は未実装のためCR0.TSを立て、ユーザーの使用は#NMで終了させます。

タスクごとに独立したPML4と下位ユーザーテーブルを所有します。コードはRX、スタックはRW/NX、
共有元はRO/NXです。コードとimmutable共有元のHHDM別名もRO/NXです。
上位PML4エントリはカーネルから借用し、全てsupervisor権限を保持します。
**ユーザーrootの生存中、共有する上位PML4の構造は固定し、カーネル側テーブルを解放しません。**
既存HHDMのleaf保護変更は共有下位テーブルに反映されます。ユーザーrootの破棄は所有する下位表と
rootだけを対象にし、借用するカーネル表には触れません。

trap入口はGPR・例外フレーム・実CR3・CR2を保存し、CLD後、カーネルCR3とカーネルスタックへ
戻ってからRustへ制御を返します。ユーザー空間のPTE編集・フレーム解放中は全ユーザータスクが停止し、
CR3切替によって以前の非global TLBは無効です。再入時にもCR3をロードします。
leaf更新のINVLPGとこのCR3切替が単一CPUでの失効を成立させます。SMPのTLB shootdownはありません。
ユーザーの通常例外は当該タスクを終了し、COWとして認可された書込みfaultだけ同じ命令を再実行します。
カーネル例外は診断して停止します。

### syscallとCapability

int 0x80のABIはRAXが番号、RDI/RSI/RDXが引数、RAXが結果です。失敗はu64::MAXを返します。
他のGPRは保存します。呼び出し元task IDは実行中のカーネル状態から決定し、ユーザー引数を信用しません。
ユーザーポインタを間接参照するAPIはありません。マップ先VAも専用4 MiB範囲内に制限します。

| 番号 | 操作 | 引数／動作 |
| --- | --- | --- |
| 1 | map | handle, VA, READまたはCOW。初期PTEはRO/NX |
| 2 | yield | phase, checksum。試験用チェックポイントと協調切替 |
| 3 | release | handle。結び付いた写像を先に除去 |
| 4 | revoke | handle。対象オブジェクトの全Capabilityと全写像を失効 |
| 5 | grant | handle, 生存中の宛先task, 権利の部分集合 |
| 6 | whoami | カーネルが決めた呼び出し元。引数は無視 |
| 7 | unmap | handle, VA。私有コピーも破棄 |
| 8 | exit | status。全Capability・写像・固有ページ・下位表を回収 |
| 9 | create | 長さ, 初期化バイト。immutableオブジェクトのhandleを発行 |

handleは上位16ビットを0とし、残りをgeneration:u32 / slot:u16として符号化します。
世代0・不正形式・発行されていないslot・古い世代を拒否します。別タスク表の同じ数値は
自タスクのentryにしか作用しません。内部object IDだけでは権限を得られません。
権利はREAD=1、MAP=2、SHARE=4、REVOKE=8、COW=64です。WRITE=16とEXECUTE=32は拒否します。
作成者はOWNER=79を持ち、SHAREで部分集合だけを付与できます。
createは長さ0、4 MiB超、初期化値255超を拒否し、末尾の未使用バイトはゼロにします。

### COW、別名、寿命

COWはimmutable共有元に対する**写像単位の私有スナップショット**の権利です。
READ+MAP+COWを確認して最初は共有元をRO/NXで写像し、present/user/write fault時に
Capabilityの世代と権利を再確認します。1つの4 KiBフレームを確保して内容をコピーし、
元の物理フレームが一致するleafだけをUser RW/NXに置換します。元の共有フレームは変更しません。
割当て／置換失敗では新規コピーを回収し、元の写像を壊さずエラーにします。

同じCapabilityを複数回COW写像しても、私有コピーは写像間で共有しません。unmap後に再mapすると
元のimmutable内容に戻ります。変更を永続化する可変オブジェクトでも、forkの実装でもありません。
私有フレームはviewが所有し、元のフレームはCapability参照が保持します。
release / unmap / revoke / タスク終了はPTEとTLBの到達性を先に除き、その後に私有コピーと
参照が尽きた元フレームをゼロ化・回収します。revokeは書込み済みの私有写像も対象です。
タスク終了はユーザーコード・スタック・秘密ページ・ユーザー専用ページテーブルも回収します。

### 容量と会計

`ObjectStore<B, N>` に拡張し、M4はN=1,024（最大4 MiB/オブジェクト）、4タスク表、
各16 Capability、8オブジェクト、32 viewを持ちます。固定配列は331,584バイトで、
スタックに作らず静的初期化します。この値はobject storeの管理配列であり、CPU/runtime状態などを含む
カーネル全体の管理量ではありません。カーネル画像の予約領域に含まれるためusableプールに二重加算しません。
M3互換のN=16 storeもCOW用管理欄を持つため、現行ビルドでは9,024バイトです。
下のM3節の4,928バイトはM3完成時の履歴値です。

実QEMUデモの共有時はpayload 1,024物理フレームに対して2,048ユーザー仮想ページです。
別にコード・スタック・秘密ページ5フレーム、ユーザーページテーブル16フレームを使います。
1ページへの2バイト書込み後は私有コピー1フレームだけが増え、payloadは1,025フレームです。
このM4デモのfile-backedページとcompressed bytesは0です。共有仮想サイズ8 MiBを物理使用量にしません。

### 検証の境界

実CPL3・異なる実CR3、全1,024ページの物理一致、ユーザーによる全4 MiB checksum、COW後の
片側だけの変更、RO/NX/supervisor/他タスクの私有ページの隔離、I/Oと不正ゲートの拒否、
CREATE・権限縮小・stale handle・失効後のfault・各タスクの回収を検査します。
主タスク2つとtask slotを再利用する12ケースで、正常終了1回・fault終了13回です。
成功判定はマーカーだけでなく数値・例外・資源収支を外側の検証器でも照合します。

COW割当て失敗は決定的な注入で、自然発生のメモリ不足ではありません。
汎用ユーザーELFローダ、タイマー割り込み／プリエンプション、SMP、FPU利用、外部入出力は未実装です。
組込みフィクスチャによる検証は、任意の悪意あるプログラムに対する可用性や包括的安全性の証明ではありません。
正確なログ・ハッシュ・制約は [M4-VERIFICATION.md](M4-VERIFICATION.md) を参照してください。

## M3の実装境界（M3時点の記録）

`src/object.rs` は依存クレート・ヒープ・unsafeを使わない固定容量のオブジェクト管理です。
`src/m3.rs` が実フレーム管理とページテーブルへ接続します。
単一CPU、割り込み無効、操作を直列に実行する前提です。SMP向けのロックやTLB shootdownはありません。

### Capabilityと権利

各論理タスクは独立したCapability表を持ち、公開ハンドルはslot:u16とgeneration:u32です。
全操作で呼び出し元の表と世代を照合します。内部のobject slot/generationをユーザーの権限にしません。
同じ数値のハンドルが別タスクにも存在することは正常で、それぞれ自分の表のentryだけを参照します。
release/revoke後に世代を進め、u32上限に達したCapability/object slotは世代0として永久に退役させます。
参照数・サイズ・仮想アドレスの加算も検査し、折り返しを認めません。

権利はREAD、MAP、SHARE、REVOKEです。作成元は4つを持ち、SHARE保有者だけが自身の権利の
部分集合を別の表へ付与できます。READはlength照会、READ+MAPはR/NX写像に必要です。
WRITE/EXECUTE要求はimmutableオブジェクトでは拒否します。権利の増幅はありません。
REVOKEを付与された主体も、作成元を含むオブジェクト全体を失効できます。
派生Capabilityの木や部分木だけの失効は実装しません。

現在のAPIのtask引数は**信頼されたカーネル呼び出し元が供給**します。
ユーザーコードが自分でtask IDを指定できるsyscall APIではありません。
M3にはユーザープロセスが存在しないため、悪意あるring-0コードからの隔離は主張しません。
M4でカーネルが現在タスクを決定し、独立ページテーブルとring-3の権限境界を接続します。

### immutable backingと寿命

createはページを確保し、全ページをゼロ化して入力データをコピーした後、公開前にsealします。
入力コピーは生成時に1回あります。grant/mapではpayloadをコピーせず、同じ物理フレームを使います。
各新規payloadの唯一の初期別名であるHHDMもR/NXに変更し、INVLPGを実行します。
その後の全オブジェクト写像はR/NXであり、書込み可能な別名を残しません。
sealはM2の任意フレームへ公開するAPIではなく、Store所有の新規payloadだけを対象にします。

Descriptorはページ配列、バイト長、ページ数、Capability参照数を持ちます。
各写像は作成したCapabilityに結び付き、releaseはそのCapabilityの全写像を先に除去します。
従って生きている写像には必ずCapability参照があり、写像だけが残ってpayloadを早期解放することはありません。
unmapは写像開始VAを指定する全オブジェクト単位の操作で、Capability自体は保持します。
他Capabilityが作った写像をunmapすることはできません。

revokeは対象オブジェクトの全Capabilityをreleaseします。最後の参照を除去するときには
全PTEの除去とローカルTLB無効化が完了しています。その後、HHDMをprivate RW/NXに戻し、
payloadをゼロ化してからアロケータに返します。権限トークンの削除だけで失効扱いにはしません。
作成元が通常releaseした場合は共有先の参照を保ち、最後の共有先のreleaseで回収します。
StoreのDropも同じ順序で全Capabilityを回収します。

create途中の割当て・seal失敗では取得済みページを全回収し、ハンドルは発行しません。
map途中の失敗ではその呼び出しが作った写像だけを逆順に除去し、既存写像・payload・Capabilityを保ちます。
backendのunmap/destroyは所有権不変条件の下で失敗しない契約です。破綻時は停止し、続行して解放しません。

### 容量と会計

- 4論理タスク表、各16 Capability、8オブジェクト、32写像レコード。
- 1オブジェクト最大16ページ（64 KiB）。長さ0は拒否。末尾ページの未使用部分は0。
- カーネル統合試験では各タスクに2 MiBの専用試験VA範囲を割り当て、範囲外を拒否します。
- metadata配列はx86-64で4,928バイト。現在は既存64 KiBカーネルスタック内にあり、
  payloadの物理フレームとは別です。backend・入力バッファ・一時変数・コードはこの数値に含みません。
- QEMUの4,097バイトpayloadは2物理フレーム、2写像先で4仮想ページです。
  追加ページテーブルは4フレーム。仮想写像の合計を物理payloadとして加算しません。
  file-backed、compressed bytesはいずれも未実装で0です。

### 検証とM4との境界

ホストテストは権限の縮小、stale/不正ハンドル、世代/参照数上限、二重解放、容量、部分失敗、
1,000回の作成・共有・回収、Dropを検査します。QEMUではM1/M2試験に続き、実PTEの物理アドレス一致、
全payloadとゼロ末尾、HHDM別名のRO、9回のCPU保護例外、全フレーム・ページテーブルの回収を検証します。
作成失敗は実backendへの決定的な割当て失敗注入、map失敗は実PTEの2ページ目の衝突を使います。

この試験の2つの論理タスクは同一supervisorアドレス空間にあります。独立したring-3タスクの
セキュリティ、syscall、COW、並行アクセス、実4 MiBデモ、性能優位性は未検証・未実装です。
数値とログは [M3-VERIFICATION.md](M3-VERIFICATION.md) に保存します。

## M2の実装境界（M2時点の記録）

M1の起動診断後に `src/m2.rs` が次の処理を行います。
以下のM1節は履歴として残してあり、M2ではページテーブル・スタック・例外処理の範囲が変わります。

### 物理フレームと会計

`src/frame.rs` は4 KiBごとのeligible / allocatedビットマップを使います。
usable領域の内側に完全に入るページだけを候補にし、重なりがあれば予約領域を優先します。
未知種別、ACPI、ブートローダ、カーネル、フレームバッファは全て割当て対象外です。
物理アドレス0も除外します。メタデータはカーネルのBSSにあり、自己割当てされません。
不正な領域、容量超過、非整列解放、予約領域の解放、二重解放はエラーになります。

現在の静的容量はusable領域の終端が1 GiB以下、記述子4096件以下です。
超過時はパニックして停止します。予約MMIOの高位アドレスが1 GiBを超えるのは構いません。
初期化時のeligibleマップは不変で、allocatorはpayloadを自動ゼロ化しません。
ページテーブル・スタックは使用前にゼロ化します。新しい信頼境界へ渡すpayloadのゼロ化はM3/M4で必要です。

`M2FRAMES` はusableプールのtotal / allocated / freeをフレーム数で表示します。
allocatedはページテーブルと新スタック、試験中のpayloadを数えます。
ビットマップ65,536バイトと試験用ビットマップ32,768バイトはカーネル画像内の
メタデータです。メモリマップ保存配列は98,304バイトを予約します。
これらは `MEMKIND kernel_and_modules` の範囲内にあり、別の割当てとして二重加算しません。
構造体・IDT・コードなどの残りもカーネル画像に含まれます。
`mappings` は仮想ページ対応数であり、追加の物理RAM消費量ではありません。
旧ブートローダのページテーブル等は `bootloader_reclaimable` に保持したままです。

### ページテーブル、保護と寿命

`src/paging.rs` はカーネル所有の4段・4 KiBページテーブルです。
huge page、PCID、LA57、ユーザーページ、共有ページテーブルは未対応です。
map途中の確保失敗では新設した表だけを逆順に回収します。既存mapの上書きは拒否します。
unmapはPTEを除去し、`INVLPG` 後に空になった子テーブルを回収してpayloadの物理アドレスを返します。
payloadの寿命は呼び出し側の責任です。生フレームのreleaseはユーザーAPIではなく、
同じ物理アドレスの再利用後に古い呼び出し元がreleaseすることを防ぐ世代機構はまだありません。

新しいCR3はusable・bootloader・kernel領域のHHDMと、カーネル画像、新スタックを写像します。
MMIOやフレームバッファを無差別に写像しません。物理ページ0も写像しません。
カーネルコードはRX、rodataはR/NX、その他のデータはRW/NXです。
コードとrodataのHHDM別名もR/NXにして、書込み可能な実行コード別名を残しません。
CR0.WPとEFER.NXEを有効化し、旧グローバルTLBを無効化してCR3を切り替えます。
このポリシーは現在のカーネル写像に対するもので、任意の新規写像の物理別名を汎用的に追跡する機構ではありません。
またROの試験payloadには信頼されたカーネルのRW/NX HHDM別名があり、immutableオブジェクトの実装ではありません。

64 KiBの新スタックは前後を未写像ページで挟みます。単一CPU・割り込み無効の前提で動作します。
SMPのTLB shootdown、参照カウント付き写像、ユーザーへの権限付与はM3/M4以降です。
旧スタックと旧GDTを含むbootloader-reclaimable領域は解放していません。

### 例外による検証

`src/faults.rs` はIDTのページフォルト（vector 14）だけを実装します。
試験時はCR2・エラービット・例外RIPが全て一致した場合だけ復帰します。
ガード2箇所、カーネルコードとHHDM別名への書込み、unmap後の読取り、RO payloadへの書込み、
NX payloadの実行をCPUに試行させます。想定外のページフォルトは診断を出しコード35で停止します。
TSS/ISTとdouble-fault専用スタックは未実装です。ガード領域の未写像は実証しましたが、
本当のスタック使い切りからの安全な診断・復帰は実証していません。
他のCPU例外も汎用的には捕捉せず、異常終了／外側のタイムアウトで検出します。

## M1の実装境界（2026-11-08）

- Rust 1.96.0、`x86_64-unknown-none`、`no_std` / `no_main` のELFカーネル。
  外部Rustクレート、ヒープ、OSランタイムを使用しません。
- Limine v8.7.0の固定コミット、ベースリビジョン3、OVMFによるUEFI64起動。
  最小限の `repr(C)` ABIを `src/boot.rs` に定義し、必須応答の欠落はパニックにします。
- `linker.ld` は高位アドレス `0xffffffff80000000` に、要求領域RW、コードRX、
  読取専用データR、データRWの各ELFロードセグメントを配置します。
  ページテーブルはLimineのものを保持しています。HHDMの書込み・実行可能な別名を
  解消していないため、システム全体のW^X達成を主張しません。
- ブートCPUのみを使い、割り込みを無効のまま維持します。AP起動、独自IDT、
  例外処理、スケジューラはありません。CPU例外はRustパニックとして捕捉できず、
  異常終了または外側のQEMUタイムアウトで検出します。
- COM1は115200 baud、8N1。シリアル送信待ちは有限です。
  `isa-debug-exit` のポート `0xf4` に成功時 `0x10`、パニック時 `0x11` を書き、
  QEMUの終了コードはそれぞれ33、35になります。デバイス不在ならCLI/HLTで停止します。

### メモリ集計と寿命

Limineがファームウェア情報から構成した物理メモリマップを読み取ります。
生のEFIディスクリプタを独自に解析する実装ではありません。
各領域の物理アドレス・長さ・種別、および種別ごとの領域数・バイト数を表示します。
`usable_bytes` は種別0のみ、`reserved_bytes` はそれ以外の合計です。
後者にはACPI、ブートローダ、カーネル、フレームバッファ、未知の種別も含めます。
別途 `MEMKIND name=reserved` はLimine種別1のみを表します。

この集計は物理アドレス範囲の合計で、実消費RAM量ではありません。MMIO予約範囲を
含むため、QEMUに指定したRAM量より大きくなり得ます。空きフレーム数、割当済み
フレーム数、メタデータ消費量などのアロケータ統計はM2以降です。
M1はどの領域も割り当て・解放しません。

ブートローダを信頼し、応答ポインタと記述配列は有効かつ整列済みと仮定します。
NULL、領域数の上限4096、ゼロ長、アドレス・集計オーバーフローは検査します。
この検査は悪意あるブートローダの任意ポインタを安全化するものではありません。
応答、初期スタック、GDT、ページテーブルを含むbootloader-reclaimable領域は全て
保持します。M2で再利用する前に、参照先の退避と各構造の置換を完了する必要があります。

### 検証と未実装範囲

ホスト単体テストは集計と異常値を検査します。QEMUテストは実シリアルログを読み、
各領域から独立に再集計し、UEFI種別・起動マーカー・成功マーカー・終了コードを照合します。
パニックとタイムアウトにも実QEMUでの検査があります。
Capability、ring-3の分離、共有物理ページ、COW、失効は後続の研究対象です。
以下はその将来設計であり、M1で実装済みという意味ではありません。

## Objective
Evaluate whether treating shared, immutable or copy-on-write memory objects as first-class kernel resources can reduce *resident physical memory usage* and copies for particular multi-task workloads without degrading isolation, latency or reliability.

This is not the false premise that OSes do not already share libraries, mmap file pages, or COW frames. Distinguish genuinely new behaviors, integration benefits and benchmark workloads.

## Layers

1. **Boot and hardware:** Limine boot information; serial output; paging; interrupts; QEMU.
2. **Frame and address-space management:** protected physical frame allocator; page tables; translation/permissions; memory map reserved frame exclusions.
3. **Capability kernel:** task-local handle table; object rights (`READ`, `WRITE`, `MAP`, `SHARE` as needed); generation-safe lookup; grant/delegation; explicit lifecycle and revocation.
4. **Memory objects:** kernel-internal ObjectId; size, kind, physical-frame backing, immutable/sealed flag, COW state, mapping/refcount information; separate metadata from payload.
5. **Task/user API:** minimal ring-3 task sandbox with syscalls for object creation, grant, mapping, clone-on-write, release; demonstrate sharing without duplicate payload frames.
6. **Policy/optimization:** read-mostly sharing, optional compression/deduplication, observed usage, and baseline comparisons.

## Critical security invariant
A capability is an authorization to *request* a mapping. Page tables (and eventually another real protection mechanism) must enforce access on every CPU read/write. A raw numerical ID must never confer rights by itself. Stale handle + reused slot must not access a newly allocated object.

## Stage-1 model (hybrid by design)
Untrusted native processes have separate address spaces and share physical pages by mapping the same backing frames at potentially different virtual addresses with independently checked rights. This tests object-centric policy while retaining hardware-enforced protection.

Benefits: implementable on commodity x86-64, debuggable, comparable to baseline mmap/COW. Limitations: still requires page tables and TLB management; control flow and native raw pointers remain process-local.

## Experimental Stage-2 alternatives
- **Single address space with verified/sandboxed code:** language runtime, restricted instruction set and/or WASM-like validation; needs defense against native unsafe code and device DMA.
- **Capability hardware:** CHERI-inspired extensions, not assumed present on ordinary x86-64 machines.
- **Hybrid:** shared region mapped at common virtual addresses with task permissions, retaining per-task page-table roots.

Document security assumptions, performance consequences and hardware feasibility before choosing one. A plain shared virtual address space plus software handle checks is NOT an acceptable secure isolation implementation for arbitrary native code.

## Initial object lifecycle
`create -> capability -> map -> share/grant -> map in another task -> optionally clone COW -> unmap -> revoke/release -> reclaim`.

Questions requiring clear answers: How does revoke handle existing mappings? What happens to active readers and writers? How are COW faults handled? When may physical frames be freed? How do permissions and counters behave under concurrency?

## Demonstrations
1. Two isolated tasks map a 4 MiB immutable object backed by identical physical frames; second mapping does not add a second 4 MiB of payload frames.
2. A writer requests COW; changing one 4 KiB page incurs one page copy rather than duplicating all 4 MiB (subject to page granularity).
3. Unauthorized mapping fails; read-only mapping cannot be written; stale handles are rejected; proper unmapping/revocation prevents access.
4. Metadata overhead and mapping/TLB cost are included in the physical memory and performance measurements.

## Baselines
Compare separate anonymous allocations, ordinary shared mappings and COW where implemented. Use identical workloads for throughput/latency/peak frames. Do not compare against an artificially inefficient baseline alone.

## Metrics
- Allocated physical frames/peak frames (exclude free cache only with clear accounting)
- Payload-sharing ratio and metadata overhead
- Copies and page faults
- Create/map/unmap latency, COW fault latency
- Throughput under 1, 2, 4, 8 simulated tasks
- CPU cost and tail latency where measurements permit

## Historical perspective
Microsoft Research Singularity explored software-isolated processes with language guarantees; it is not evidence that user objects can be indiscriminately shared safely. seL4 offers capability-mediated memory sharing while retaining virtual-memory mappings. Position this project honestly relative to predecessors.
