# 技術判断記録

## M12: host固定を保った失効制御

1. Sessionへ任意のhost操作closureを渡す仕組みは作らず、RevocableHost traitの限定操作にする。
2. Launchにowner handleを保持し、revoke時にtakeする。重複要求をfalseにし、通常stopと
   object失効を区別する。既存ObjectStoreのPTE/世代/参照管理を再利用する。
3. 新しい外部moduleで再読取りと再起動を検査する。古い値の消去やSMP中の失効を保証せず、
   native taskのページング隔離を維持する。元moduleの予約ページは引き続き保持する。

## M11: 外部codeと信頼された初期権限設定を分離

1. M9の無権限デモを維持し、同形式の別boot moduleをM11専用入口にする。署名認証とは呼ばない。
2. 固定3programと非実行owner slotの専用storeに限定。manifestはkernel内のREAD許可配列で、
   external inputにtask IDや権利maskを宣言させない。r1は実task-local handleの受渡しだけに使う。
3. Launchが準備から所有者解放までをRAIIで管理し、Sessionは実行taskを回収する。
   busy storeを引き受けず、失敗で無関係の資源を消さない。Dropのabort/forget制約はM10と同じ。

## M10: 外部実行の所有関係を先に固定

1. 初期権限付与に先立ち、M8の明示的shutdownとhost固定の契約をSession型へ閉じ込める。
   既存Schedulerの挙動は変えず、外部コードの実行経路だけSessionへ移す。
2. 検証済みBatchをborrowし、権限hostをexclusive borrowする。実行中のコード変更やhost付替えを
   compile-failで検査する。通常終了/中止はDropからshutdownし、二重stopを防ぐ。
3. panic=abortやmem::forgetでDropを保証する主張はしない。低水準Schedulerも既存研究fixture用に残す。
   汎用kernel回復機構の代わりではなく、通常制御フローの寿命の改善として扱う。

## M9: 全件検証する外部コードと所有期間

1. Limineのmodule機能でISO内の独立ファイルを読み、カーネルELF再コンパイルなしで入力を変更できる。
   実ファイル由来のfingerprintとSHA256を記録し、同じELFで42→99を実起動して照合する。
2. version付きの小さいコンテナを自作し、最大4件・code合計512 byte・生涯fuel合計上限を強制する。
   汎用ELF/WASMローダを名乗らず、既存8命令の脅威モデルを広げない。
3. 全件検証後にだけ所有Batchを返す。Programのimmutable borrowで実行中のcode破棄を禁止する。
   不正な最後のprogramでも先行programを起動しない。
4. 初期Capabilityは空とする。外部code内のhandle値を権限として採用しない。
   初期object共有は後続の信頼されたlaunch manifest設計へ分離する。
5. 元Limine moduleページは保持し、実行用bufferのclearと区別する。FNV診断値を署名や認証と呼ばない。
   根拠と制限は[M9-FORMAT.md](M9-FORMAT.md)に記載する。

## 2026-11-09: M8は言語拡張より先に実行資源を制限

1. 外部bytecodeを受け入れる前に、YIELDなしloopの占有と複数Contextの公平性を扱う。
   固定4slot、round-robin、1〜64命令quantumを採用する。nativeのページ保護は変更しない。
2. 生涯燃料とdispatch命令数を分離する。SUMは既存どおり1+lengthを呼出し前に払い、
   quantumは命令境界で作用する。同期host処理の途中中断・時間上限を主張しない。
3. batch累積燃料の上限を設け、終了・取消で返却しない。slotはbatch中再利用しない。
   動的task ID再利用や追加世代表を導入せず、固定容量の検証可能な段階に限定する。
4. 認可hostはschedulerが選んだtask IDを使う。HALT/fault/cancelでrelease_taskを呼び、
   不正な1taskが他taskの権限や実行を破壊しないことを検証する。
5. 失効を強制中断と再開の間へ挟み、次SUMでの認可と実物理ページの共有を検査する。
   同じstore・HHDM RO検証を再利用し、新しい物理アクセス経路を増やさない。
6. 新しい依存・host設定変更は不要。検証した範囲をM8報告に記録し、外部loaderの
   code保持quotaとversion付き入力形式を次の優先課題とする。

## 2026-11-09: M7の限定的な言語隔離

1. 独立pagingをnativeタスクに維持し、別経路として制限付きインタプリタを追加します。
   WASM runtimeや新CPUを導入せず、検証する命令・host API・資源上限を監査できる範囲に限定します。
2. 実装前に [脅威モデル](M7-THREAT-MODEL.md) を定義しました。インタプリタはCPL0の信頼基盤であり、
   その実装バグをハードウェアが隔離するとは扱いません。任意native命令は一切取り込みません。
3. ObjectStoreにREAD権利を検査する同期checksum APIを加え、全アクセスで世代・範囲を再検査します。
   プログラムへnativeポインタを返さず、ObjectStoreとコードをimmutable borrowします。
4. 命令数・scratch・SUM長に上限を持ち、燃料を命令と読取りバイト数に課します。
   燃料はyieldで再付与せず、失敗後のContextは停止します。無限loop拒否と実時間保証を区別します。
5. M6のnativeフィクスチャと同じ入力を用い、物理frame traceとPTEを各経路で確認します。
   未使用秘密ページを含む既存対照群であること、bytecodeのbulk host処理との非対称性を明記します。
   管理領域・コード増分を含む画像とallocatorピークを報告し、制限のないアプリの省メモリ化とは主張しません。

## 2026-11-09: M6の対照群と測定境界

1. eager-privateだけでなく、Capability管理を通らない通常共有／COWも対照群にします。
   いずれも同じページテーブル・CPU遷移を使い、raw経路は信頼された有限試験だけに限定します。
2. 16／256 KiB、dirty 0／1／全ページの22条件、各ウォームアップ1回＋本測定8回を採用します。
   方式の順序を回転し、生データを全保存します。8件からtail latencyや統計的有意差は主張しません。
3. 成功経路のシリアル診断は計時区間の外へ出します。map/createはカーネルAPI時間、
   read/write_readは実ユーザー遷移込み、COW handlerはtrap入口／復帰を除く内訳です。
   フェンス付きゲストTSCを継続使用し、実CPUサイクル・秒に換算しません。
4. 実allocatorの全割当てでピークを追跡します。カーネル画像全体とプールのピークを合算し、
   画像内の固定メタデータや仮想写像数を二重加算しません。ファームウェア等は対象外と明示します。
5. M5ポリシーは専用容量を維持して回帰検証し、M6の対照群へ混ぜません。M6で通常共有とobject共有の
   payloadが同じになることも結果です。既存技術に対する未測定の優位性を結論にしません。
6. 198試行を含む通常起動の上限を90秒に延長します。検証器は順序・会計・実タスク・checksum・回収を
   必須にし、成功マーカーだけでは通しません。依存関係やホスト環境は変更しません。

## 2026-11-09: M5の任意の重複排除と圧力時圧縮

1. 既存のObjectStoreの権限とobject identityを維持し、専用Backend adapterが内容と物理backingだけを集約します。
   別objectのrevokeを連動させず、実CPL3の読取りで残存objectが有効なことを確認します。
2. 32 backing・64ページ写像・128圧縮バイト/backingに固定し、未知の動的メタデータを隠しません。
   合計12,728バイトのstore・policy・圧縮予約領域を常に計上します。共有は全バイト比較で確認します。
3. ハッシュ探索は8連続missでepoch終了まで停止します。圧縮に失敗したimmutableページも再試行しません。
   まず境界の明確なポリシーにし、周期的な再適応や複雑な予測器は追加しません。
4. 未写像のimmutableページだけを圧縮し、map時に復元・sealしてからPTEを作ります。
   写像中のページの追出しや圧縮page faultは実装せず、M4のTLB・COW・寿命不変条件を維持します。
5. 圧力は実free framesとwatermarkの比較で判断し、試験ではwatermarkを意図的に高くして経路を通します。
   自然なメモリ枯渇を再現したとは主張しません。ランレングス圧縮は予約128バイト以内に収まる場合だけ採用します。
6. 3負荷で各ON/OFF 3回を同一バイナリで実行し、QEMU TSC tickの生データとmin/median/maxを保存します。
   フェンス付きの読取りは [Intel SDMのRDTSC記述](https://cdrdv2-public.intel.com/774492/325383-sdm-vol-2abcd.pdf)
   を参照しました。ゲストtickを実CPUサイクルや校正済み時間へ換算しません。
7. M5の比較は同じ予約メモリを持つadapterのON/OFFです。素のprivate／shared／COWベースライン、
   診断出力を分離した遅延、充分なサンプル数、汎用的な性能解釈はM6で扱います。

## 2026-11-09: M4の実ユーザータスクとCOW

1. GDT/TSSとIRETQ、DPL3のint 0x80を採用します。SYSCALL/SYSRET固有の入口処理を増やさず、
   CPUが作る例外フレームを共通の保存・復帰経路で扱います。syscall呼び出し元はカーネルが決定します。
2. タスク固有の下位ページテーブルと、supervisor専用の共有上位カーネル写像を使います。
   上位PML4構造の固定・カーネル表の長寿命・単一CPU・CR3によるTLB無効化を不変条件にします。
3. COWはWRITEとは別の権利です。immutable元とそのHHDMをROに保ち、書込みfaultで
   当該viewの当該ページだけをRW/NXの私有コピーへ置換します。unmapは私有変更を破棄します。
4. ObjectStoreのページ容量をconst genericにし、M4は4 MiBに拡張します。管理配列331,584バイトを
   静的に確保し、物理payloadや仮想写像と分けて報告します。初期化値によるCREATEを用意し、
   巨大な入力バッファや未実装のユーザーポインタ読取りを必要としない境界にします。
5. 汎用ELFローダより先に、組込みの実ユーザー機械語で特権遷移・共有・COW・例外を検査します。
   GPRは保存し、未対応のFPU/SIMDはCR0.TSで拒否します。協調実行のため可用性保証はありません。
6. 全4 MiBのPTE一致とユーザーchecksum、1ページコピー、12ケースの正常／違反動作、
   失効・タスク終了後の全回収を128/256 MiBで検証します。性能比較はM5/M6に残します。

## 2026-11-08: M3のCapability object store

1. まずimmutable専用にして、WRITE/EXECUTEを拒否します。作成時だけ初期化し、HHDMもsealして
   RW別名を除去します。COWはM4の実タスク・書込みfaultと一緒に追加します。
2. Capabilityのtask-local slot/generationと内部object slot/generationを分離します。
   上限到達時はslotを退役させ、巻き戻った世代で古いハンドルを復活させません。
3. 写像をCapabilityに結び付け、releaseで写像を先に消します。REVOKEはオブジェクト全体の
   Capabilityと写像を消す強い操作とし、派生木管理を今回の必須条件にしません。
4. 固定容量の配列と単一CPUの直列操作に限定し、失敗時の回収経路を検査可能にします。
   生フレームAPI・task ID・backendは信頼されたカーネル専用です。
5. ホストモデルの失敗注入と実QEMUのPTE・CPU例外を併用します。
   同一CR3内の2つの試験VA範囲を、独立ユーザープロセスの隔離実証とは扱いません。
6. payloadの2物理フレーム、追加ページテーブルの4フレーム、4仮想写像、既存スタック上の
   4,928バイトの管理配列を分けて報告します。性能・メモリ節約の比較ベンチマークはM6です。

## 2026-11-08: M2の物理フレームとページ保護

1. ビットマップのeligibleとallocatedを分離し、予約領域と解放済み領域を区別します。
   メタデータをBSSに固定し、まず1 GiBまでを検査可能な範囲とします。動的拡張やNUMAは後続です。
2. 既存ページテーブルのhuge pageを部分修正する代わりに、全て4 KiBの独自テーブルを作ります。
   テーブルの物理フレーム数を明示し、単純さのために必要な管理コストを隠しません。
3. コードのHHDM別名もRO/NXにし、単にELFのRXセグメントだけでW^Xを主張しません。
   現時点の全カーネル画像ページを、通常のVAとHHDM側の両方で照合します。
4. ページフォルトは固定条件のプローブのみ復帰可能にします。無関係な例外を握り潰さず、
   予期しないアクセスは停止させる専用ISOでも検証します。
5. ブートローダメモリは保持します。旧GDTを利用しており、TSS/IST・汎用例外基盤は未実装です。
   実際のスタック枯渇からの復帰やSMPを、ガードページの存在確認と混同しません。

ページ保護・CR0.WP・EFER.NXE・ページ構造キャッシュ無効化の根拠は
[Intel SDM Volume 3A（特に第4章）](https://cdrdv2-public.intel.com/825758/253668-sdm-vol-3a.pdf) です。
Limineからは既に固定したv8.7.0 ABIのHHDMとExecutable Address応答を追加取得します。

## 2026-11-08: M1の最小ブート経路

1. **Rust 1.96.0 + 組込み `x86_64-unknown-none` ターゲット**。
   Windowsに既存の同版コンパイラがあり、追加ターゲットでELFをクロスビルドできます。
   nightly、`build-std`、独自JSONターゲット、外部Rustクレートは不要でした。
   このターゲットのSysV ABI、red zone不使用、標準の浮動小数点・SIMD不使用は
   [Rust公式ターゲット仕様](https://doc.rust-lang.org/rustc/platform-support/x86_64-unknown-none.html)
   に従います。

2. **Limine v8.7.0、ベースリビジョン3**。
   最新追従よりも確認可能な固定仕様を選びました。
   起動バイナリは `aad3edd370955449717a334f0289dee10e2c5f01` に固定し、
   3ファイルのSHA256を記録します。メモリマップ・ファームウェア種別だけのABIを実装します。
   参照仕様は [公式PROTOCOL.md](https://github.com/limine-bootloader/limine/blob/v8.7.0/PROTOCOL.md)、
   [公式limine.h](https://github.com/limine-bootloader/limine/blob/v8.7.0/limine.h)、
   [公式CONFIG.md](https://github.com/limine-bootloader/limine/blob/v8.7.0/CONFIG.md) です。
   ABI定数・レイアウトの由来とライセンスは `THIRD_PARTY.md` に記載します。

3. **OVMF + UEFI ISO + QEMU TCG**。
   q35 / qemu64 / 1 CPUを使用し、KVMなしでWSL2から再現できます。
   ISOにはEl ToritoとUEFIのパーティション情報を含めます。
   初回ISOではLimineが起動ボリュームを特定できずフォールバックする警告が出たため、
   `-efi-boot-part --efi-boot-image --protective-msdos-label` を追加して再試験し、警告の解消を確認しました。
   BIOS用のホストディスク操作やブートローダインストールはありません。

4. **割り当てを行わないメモリ集計**。
   ブート情報の種別を失わずに保持し、利用可能なのは種別0だけとします。
   物理アドレス空間にある巨大なMMIO予約領域と実RAM使用量を区別します。
   アロケータ、ページテーブル置換、ユーザー空間の保護はM2以降です。

5. **実起動・失敗経路の両方を検証**。
   検証器はマーカーに加えて、領域別情報から再計算した総量とQEMU終了コードを検査します。
   成功33、パニック35、タイムアウトの強制終了を区別します。
   合成ログによる検証器の単体テストは、実QEMUの起動証拠として扱いません。

6. **既存の設計文書を保存**。
   初期ファイルはルート直下にあり、参照先の `docs/` がありませんでした。
   元ファイルを保存して正本への案内だけ追記し、`docs/` に仕様と実装状況を配置します。
