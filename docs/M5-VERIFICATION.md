# Milestone 5 検証報告

この文書と `evidence/m5/` はM5時点の履歴です。通常の成果物はM7のビルドで更新されました。
現在のISO・ELF・検証結果は [M7検証報告](M7-VERIFICATION.md) を参照してください。

2026-11-09 JST。任意のimmutableページ重複排除と、圧力時の未写像ページ圧縮・再map時復元を実装しました。
M5のゲートを通過しています。128 MiB・256 MiBのQEMUでそれぞれ18回のON/OFF比較を実行し、
実ring-3の読取り、COW、独立したobjectの失効、回収を検証しました。

## 生成物と再実行

正式コマンド:

```powershell
./tools/project.ps1 verify
```

全体の終了コード0、内部の全18コマンドも0です。QEMU自身の終了コードは正常33、
意図的panic／想定外fault35、2秒タイムアウト-9で、各試験が期待値を照合しています。
最後に正常版ELFへ戻し、実際に両構成で起動したISOを保持しました。

| 生成物 | パス | SHA256 |
| --- | --- | --- |
| 正常版UEFI ISO | `build/normal/shared-first.iso` | `d19a515fd13b6ffbfea8b6f663106596b8b51f8cf269953f0eb211fd906b4f49` |
| 正常版Rust ELF | `target/x86_64-unknown-none/release/shared-first-kernel` | `9a04e79726b738f0706a953e85f3d04bb2a44d18eacb4ac0a975482f4e36f1c7` |

[生成物照合記録](evidence/m5/artifact-verification.json)に元ログの場所と最終ファイルの照合結果があります。
最終ISOは両実起動のISOと一致し、最終ELFはISO作成時のELFと一致します。
Rust 1.96.0、Limine v8.7.0固定コミット、WSL UbuntuのQEMU 10.2.1 / OVMFを継続利用しました。
q35・qemu64・TCG・1 CPU、ネットワークなし・ホスト物理ディスクなしです。
依存関係の追加、commit、push、branch作成は行っていません。

## 検証結果

| 検証 | 結果 | 証跡 |
| --- | --- | --- |
| Rust整形 | 成功 | [fmt](evidence/m5/commands/02.log) |
| Rustホスト単体テスト | 36件成功 | [出力](evidence/m5/commands/03.log) |
| Clippy、警告をエラー化 | ホスト・カーネルとも成功 | [ホスト](evidence/m5/commands/04.log)、[カーネル](evidence/m5/commands/05.log) |
| Python検証器テスト | 26件成功 | [出力](evidence/m5/commands/06.log) |
| Rust releaseとISO作成 | 成功 | [ビルド](evidence/m5/commands/07.log)、[ハッシュ](evidence/m5/image-normal/hashes.json) |
| QEMU 128 MiB | M1〜M5 PASS、コード33 | [シリアル](evidence/m5/boot-128M/serial-readable.log)、[結果・全サンプル](evidence/m5/boot-128M/result.json) |
| QEMU 256 MiB | M1〜M5 PASS、コード33 | [シリアル](evidence/m5/boot-256M/serial-readable.log)、[結果・全サンプル](evidence/m5/boot-256M/result.json) |
| 意図的panic | コード35、期待した診断 | [結果](evidence/m5/panic/result.json) |
| 想定外カーネルfault | コード35、期待した診断 | [結果](evidence/m5/unexpected-fault/result.json) |
| 実QEMUの2秒タイムアウト | kill・wait、コード-9 | [結果](evidence/m5/timeout/result.json) |

正確なコマンド・cwd・終了コードは `evidence/m5/commands/*.json`、出力は対応する `.log` にあります。
QEMU引数・ISOとファームウェアのハッシュは各 `result.json` に保存しています。
最初の `cargo test --locked --offline --lib` ではテスト用Modelのcopy_privateでE0502（可変／不変借用の競合）が
発生しました。コピー元をローカル配列へ退避して修正し、上記の正式な全検証を通しました。
QEMUの予期しない起動失敗はありません。合成ログを使った検証器の単体テストは実起動証拠と区別しています。

## 実装と比較方法

ポリシーは既存ObjectStoreのBackendを包み、専用store内だけで明示的に有効化します。
内容を集約してもobject identityとCapabilityは分離したままで、別objectのrevokeには巻き込みません。
ハッシュ一致に加えて全ページ比較を必須とし、8連続missで試験epochの探索を停止します。

圧縮はseal済み・未写像・非私有のページだけです。RLE表現が予約128バイト以内に収まる場合に採用し、
元の実フレームを返却します。復元はmap時に行い、初期化とseal後にPTEを作ります。
写像中のページとCOW私有ページを圧縮せず、失敗したimmutableページを繰り返し圧縮判定しません。
これは既存の重複排除／RLEを用いた限定的なポリシー実験です。

各試験は4個の独立object×16 KiB＝16論理ページ・64 KiBを作成します。
入力は全ページ同じ0x5a、各ページが異なる一定値、決定的疑似乱数の3種類です。
入力の64-bit hashをON/OFFで照合し、各側3回、順番はOFF→ON、ON→OFF、OFF→ONにしました。
初回サンプルを除外せず全て保存しています。各RAM構成18試験、合計36試験です。

処理順はcreate、圧力なしmaintenance、圧力ありmaintenanceを2回、2実タスクでmap/read、
writerの1バイトCOW、readerの不変確認、writer objectのrevoke、readerの再読取り、全回収です。
カーネルが制御したwatermark＝実free_before+32を用います。圧縮経路を確実に通すための高いしきい値であり、
自然発生のメモリ不足やOOM回避の実証ではありません。

## メモリ結果

両RAM構成・全3サンプルで同じ値です。residentの単位は4 KiB物理フレームです。
「圧力後」はユーザーmap前、「map後」は2タスクの8仮想ページを写像した時点です。

| 負荷 | ポリシー | create後resident | 圧力後resident | 圧縮ページ／使用バイト | map後resident | COW後resident |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 同一内容 | OFF | 16 | 16 | 0 / 0 | 16 | 17 |
| 同一内容 | ON | 1 | 0 | 1 / 3 | 1 | 2 |
| 異なる一定値 | OFF | 16 | 16 | 0 / 0 | 16 | 17 |
| 異なる一定値 | ON | 16 | 0 | 16 / 48 | 8 | 9 |
| 疑似乱数 | OFF | 16 | 16 | 0 / 0 | 16 | 17 |
| 疑似乱数 | ON | 16 | 16 | 0 / 0 | 16 | 17 |

同一内容ONは15回の集約hitで16参照を1物理ページにしました。再mapでは1ページを復元して共有します。
異なる一定値ONは16ページを圧縮し、読まれる8ページだけをmap時に復元しました。
疑似乱数ONは16ページ全ての圧縮を拒否し、2回目のmaintenanceでは再判定しません。
異なる一定値と疑似乱数では8連続miss後に残り8ページのハッシュ／探索を省略しています。

管理量はON/OFFで同じです。

| 固定記憶領域 | バイト |
| --- | ---: |
| ObjectStore管理配列 | 5,184 |
| Policy管理情報 | 3,448 |
| 圧縮表現の予約領域（32×128、descriptor内に分散） | 4,096 |
| 合計 | 12,728 |

圧縮使用量3／48バイトは予約4,096バイトの内部使用量です。3バイトだけのRAMで済むとは扱いません。
圧力後の部分会計 `resident×4096 + 12728` はOFFが78,264バイト、圧縮できるONが12,728バイト、
疑似乱数ONが78,264バイトです。固定領域はカーネル画像の予約メモリで、usable allocatorへ二重加算しません。
これは当該storeの部分会計で、OS全体のコード・他のメタデータ・ブートローダを含む総RAM使用量ではありません。
OFFも同じadapterと予約領域を持ちます。素のBackendとの全コスト比較はM6です。

別にユーザーコード／スタック／秘密ページ5フレーム、ユーザーページテーブル14フレームを計上しています。
8ユーザー仮想ページは追加の物理payloadとして数えません。独立したfile-backedページは0です。

| 全体回収の検査 | 128 MiB | 256 MiB |
| --- | ---: | ---: |
| usable総フレーム | 30,039 | 62,807 |
| 各試験の開始前／終了後allocated | 89 / 89 | 153 / 153 |
| 開始前／終了後ページテーブル | 73 / 73 | 137 / 137 |
| 開始前／終了後仮想写像 | 31,358 / 31,358 | 64,126 / 64,126 |
| 終了後M5 resident、圧縮ページ、参照、写像、Capability | 全て0 | 全て0 |

M4の4 MiBデモ、権利拒否・保護例外・回収も同じ最終ビルドで再検証しています。

## 遅延と追加コスト

以下は各3サンプルの中央値、単位は**QEMUゲストTSC tick**です。
実CPUサイクル数でも、校正済みナノ秒でもありません。値の分布・全生データは各 `result.json` の
`m5.summary` / `m5.runs` にあります。タイミングの大小で試験を合否判定していません。

| RAM | 負荷 | ON/OFF | create tick | pressure tick |
| --- | --- | --- | ---: | ---: |
| 128 MiB | 同一内容 | OFF | 437,723 | 1,127 |
| 128 MiB | 同一内容 | ON | 428,099 | 51,185 |
| 128 MiB | 異なる一定値 | OFF | 543,313 | 940 |
| 128 MiB | 異なる一定値 | ON | 566,822 | 677,306 |
| 128 MiB | 疑似乱数 | OFF | 637,144 | 1,114 |
| 128 MiB | 疑似乱数 | ON | 734,378 | 166,164 |
| 256 MiB | 同一内容 | OFF | 630,033 | 1,621 |
| 256 MiB | 同一内容 | ON | 443,838 | 52,264 |
| 256 MiB | 異なる一定値 | OFF | 695,811 | 1,605 |
| 256 MiB | 異なる一定値 | ON | 851,860 | 657,326 |
| 256 MiB | 疑似乱数 | OFF | 901,326 | 1,754 |
| 256 MiB | 疑似乱数 | ON | 853,942 | 159,836 |

createにはページ初期値の生成・割当て・seal・ON時の照合を含みます。pressureには2回のmaintenanceを含みます。
疑似乱数ではメモリ削減がなく、圧縮を採用しなくても約16万tickの圧力処理が必要でした。
ONのcreateが256 MiBで小さくなる例もありますが、128 MiBでは逆です。ホスト負荷やTCGの状態の影響を受ける
3サンプルの結果から一貫した高速化・低速化の率を断定しません。

map_read、cow_read、cleanupの生tickも保存しています。これらはユーザー実行、コンテキスト切替、
checksum、シリアル診断を含み、単独map syscall／COW faultの純粋な遅延ではありません。
追加診断を除いた測定と十分なサンプル数はM6の課題です。

## 隔離と失敗処理

各試験で異なるCR3の2タスクがCPL3で16 KiBずつ読み、checksumをカーネル側の生成値と照合します。
ONの同一内容では各対応PTEの物理フレーム一致を確認し、他の条件では別物理フレームを確認します。
全PTEはRO/NXです。writerの書込みは実#PF error 7を起こし、1ページだけをRW/NXの私有コピーへ置換します。
readerの内容は変わりません。写像中にmaintenanceしてもresident数とフレーム収支は変わりません。

writerのobjectをrevokeした後も、独立してcreateされたreader objectは実ユーザー読取りに成功します。
writerの旧VAアクセスは#PF error 4で終了します。readerは正常exitし、両タスクを回収します。
各RAM構成で36タスク起動、18回のCOW成功、18回の失効後fault終了、18回の正常exitを確認しました。

新しいRustテストは以下を検査しています。

- RLEの全値・複数run復元、非圧縮性データと不正形式の拒否。
- 内容を集約しても別objectのrevokeが相手の写像・権限を壊さないこと。
- 圧縮済み内容の集約、復元の確保／seal失敗で元データを保持し、map失敗でも参照を失わないこと。
- 写像中ページ・私有COWの除外、COW置換失敗時の新規コピー回収。
- 強制したハッシュ衝突時の全バイト比較、8 missのbackoff、OFF動作。
- 圧力しきい値、圧縮拒否の再試行抑止、容量上限と全回収。

検証器は全18ペア、入力hash、物理／仮想／予約領域会計、CPL3とCR3、checksum、
COW／revoke／exit、正の測定値を必須にし、欠落・不正会計・余分な例外等を拒否します。

## 未検証・適用範囲

- 自然発生の圧力、実OOM、長時間の圧縮／復元の繰返し、効果的なwatermark選定は未検証です。
  復元OOM・seal／map失敗・COW置換失敗はホストモデルによる注入であり、実QEMUでの復元OOMではありません。
- 一度mapしたページを圧力で追い出す機構、圧縮ページへのCPU faultからの復元、動的圧縮arenaはありません。
- 小さい固定store内の比較です。M4の4 MiB全体や任意サイズでM5を有効化した試験ではありません。
  32 backing（私有コピー込み）・64ページ写像を超えると容量エラーです。
- 本物のJPEG/ZIP等の圧縮済みファイルは未試験です。非圧縮性の代表として決定的疑似乱数を使いました。
  RLEが128バイトに収まらないページは、他方式で圧縮できる場合でも拒否します。
- 同一ポリシードメインの遅延・容量から重複を推測される可能性を隠す機能はありません。
  任意の相互不信タスク間へ自動適用する設計ではなく、明示的な試験store内に限定しています。
- 単一CPU・IF=0・協調実行です。SMP、プリエンプション、汎用ユーザーELFローダ、FPUコンテキスト保存、
  実機／KVM／BIOS／Secure Bootは引き続き範囲外です。包括的な安全性証明や既存OSへの性能優位性は主張しません。
- サンプル数3と診断込みの時間計測は、ベンチマーク報告の完成形ではありません。
  private／通常共有／通常COW等との公平な比較、peakの連続追跡、throughput、純粋な操作遅延はM6で扱います。

## 変更ファイルと次のM6

Git管理がないため、今回作業したファイルを明示します。

- 新規: `src/policy.rs`、`src/m5.rs`、`src/m5_user.S`。
- 更新: `src/lib.rs`、`src/object.rs`（任意Backendの会計・maintenance・初期化入口）、
  `src/m4.rs`（実ユーザー実行経路の再利用とM5呼出し）。
- 新規: `tools/m5_validation.py`、`tools/test_m5.py`。更新: `tools/qemu.py`。
- 更新: `README.md`、`docs/ARCHITECTURE.md`、`docs/ROADMAP.md`、`docs/BUILD.md`、
  `docs/DECISIONS.md`、過去M1〜M4報告の最新成果物への案内。
- 新規: 本報告と `docs/evidence/m5/`。元ログは `artifacts/`、生成物は `build/` と `target/`。

[最終ソースSHA256一覧](evidence/m5/source-hashes.json)を保存しました。M1〜M4の証跡は変更していません。
次のM6では素のprivate／通常共有／COWベースラインを用意し、入力・実行順・サンプル数・計時範囲を揃え、
peak物理フレーム、全管理量、fault／コピー数、操作遅延とthroughputを報告します。
