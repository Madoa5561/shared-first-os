# Windows + WSL2でのビルドと起動

検証構成はWindowsホストのRust 1.96.0 / MSVCツールチェーンと、
WSL2 Ubuntu 26.04のQEMU 10.2.1です。仮想CPUはTCGを使用し、KVMや
入れ子のハードウェア仮想化を要求しません。PowerShell 7で実行します。

## 初回のみの準備

既存環境の変更を伴うため、以下の導入コマンドは自動ビルドから実行しません。
この作業ではユーザー承認の上、Rustの追加ターゲットとWSLパッケージを導入しました。
Rust、PowerShell、WSL2 Ubuntuは既存のものを利用しました。

WindowsにRust/MSVCがない場合は公式のrustupおよびVisual Studio Build Toolsの
「C++によるデスクトップ開発」が必要です。新規インストール手順そのものは今回未検証です。
WSL2がない場合は管理者PowerShellから `wsl --install -d Ubuntu` を実行して
必要な再起動・Ubuntu初期設定を行います。これはOS機能を変更する別操作です。

まずPowerShellで、ツールチェーンと既存WSLを確認します。

```powershell
rustc --version
cargo --version
rustup show
wsl --list --verbose
```

Rustが1.96.0なら、必要なターゲットだけを追加します。

```powershell
rustup target add x86_64-unknown-none
```

Rustの版が異なる場合は、既存のデフォルトを変更せず固定版を追加できます。

```powershell
rustup toolchain install 1.96.0 --profile minimal --component rustfmt,clippy --target x86_64-unknown-none
```

その場合、以降の検証は `rustup run 1.96.0 pwsh -File ./tools/project.ps1 verify`
のように実行します。スクリプトは `rustc` の版を検査し、異なる版なら停止します。
固定版の自動インストールや更新は行いません。

WSL Ubuntu 26.04内で、次の固定版パッケージを導入します。
他のUbuntu版やリポジトリ更新後では同じパッケージ版が利用できない場合があります。
その場合は勝手に読み替えず、版を再選定して `tools/versions.json` と証跡を更新します。

```sh
sudo apt-get update
sudo apt-get install --no-install-recommends \
  qemu-system-x86=1:10.2.1+ds-1ubuntu3 \
  ovmf=2025.11-3ubuntu7 \
  xorriso=1:1.5.6-1.1ubuntu4 \
  binutils=2.46-3ubuntu2
```

Python 3も必要です。検証環境では既にインストールされていました。
依存パッケージの全スナップショットを同梱しているわけではありません。
直接依存の版とLimineの内容ハッシュを固定し、各実行の環境・生成物を記録します。

プロジェクトのPowerShellに戻り、固定コミットのLimineファイルを取得します。
ネットワークを使うのはこの明示的な取得操作だけです。

```powershell
./tools/fetch-limine.ps1
```

`BOOTX64.EFI`、`limine-uefi-cd.bin`、`LICENSE` を `.tools/limine/` に保存し、
`tools/versions.json` のSHA256と照合します。既存ファイルが不一致なら上書きせず停止します。
これは固定した取得内容の検査であり、別経路の署名検証ではありません。

## 通常の操作

全てリポジトリのルートから実行します。

```powershell
./tools/project.ps1 check
./tools/project.ps1 build
./tools/project.ps1 run
./tools/project.ps1 test-boot
./tools/project.ps1 verify
```

`check` はRustの整形・Clippy・ホスト単体テスト・Python検証器の単体テストです。
`build` はRust ELFとUEFI用ISOを作ります。
`run` と `test-boot` はM7でも同じ有限実行です。128 MiBで起動し、
シリアルログを起動後に表示して自動終了します。常駐OSや対話シェルはまだありません。
`verify` は上記に加え、256 MiBの起動、実QEMUの2秒タイムアウトと回収、
`boot-panic` 機能を使う意図的なパニックと、`boot-unexpected-fault` による想定外の
ページフォルトの停止処理を検証し、最後に正常版ELFへ戻します。
正常版ISOは実際に起動検証したファイルをそのまま残します。
途中のコマンド失敗は隠さず、スクリプトは終了コード1で停止します。
WSL名が異なる場合は `-Distribution <名前>` を指定します。

```text
build/normal/shared-first.iso                         正常版UEFI ISO
build/panic/shared-first.iso                          パニック検証専用ISO
build/fault/shared-first.iso                          想定外ページフォルト検証専用ISO
target/x86_64-unknown-none/release/shared-first-kernel  最後にビルドしたELF
artifacts/commands-*/                                 正確なコマンド、終了コード、出力
artifacts/image-*/                                    xorrisoログと生成物ハッシュ
artifacts/boot-*/                                     実QEMUログとresult.json
```

通常の起動時間制限は90秒です（個別指定の上限180秒）。QEMUの正常コード33とパニックコード35は
`isa-debug-exit` の仕様によるもので、検証スクリプトが意図どおりの値を成功扱いします。
「起動マーカーだけ出た」「QEMUがコード0で閉じた」「パニックした」は正常起動になりません。
正常版は `M1 PASS` / `M2 PASS` だけでも不合格です。M2のフレーム収支、枯渇・回収、
7回のページフォルトとスタック位置に加え、`M3 PASS`、共有payloadと写像の別会計、
HHDMを含む保護・失効・解放の9回のページフォルト、最終回収を照合します。
さらに `M4 PASS`、実CPL3とCR3、4 MiBの物理ページ共有、1ページCOW、ユーザー側checksum、
12ケースの検査、CREATEを含むsyscallの結果、失効・タスク終了後の回収を必須にしています。
続くM5では3負荷×有効／無効×3回の18サンプル、入力hash一致、実CPL3/CR3、重複排除・圧縮・復元量、
管理領域、追加ページテーブル、COW、独立失効、回収、正のTSC測定値を検査します。
`M5 PASS` のマーカーだけ、またはM4までの成功では不合格です。
各正常起動の `result.json` の `m5.runs` に生データ、`m5.summary` に各3件のmin/median/maxがあります。
性能の閾値は合格条件にせず、試験で得た遅延の悪化も保存します。
M6は5方式・2サイズ・dirty 0/1/全ページの22条件、各1回のウォームアップ＋8回の本測定を必須にします。
198件の順序、実CPL3/CR3、checksum、ピーク会計、fault／コピー回数、全回収を照合します。
`M6 PASS` だけやM5までの成功では不合格です。`result.json` の `m6.runs` に全生データ、
`m6.summary` に本測定8件のmin/median/maxとthroughputがあります。同じフォルダの
`m6-benchmark.md` は検証済み結果から自動生成する22条件の比較表です。
M7は22攻撃ケースと、2サイズ×native／bytecodeの4条件、各warmup 1＋測定8の36試行を必須にします。
nativeのCPL3・独立CR3、bytecodeのkernel CR3・CPL0、同じ物理backing、checksum、燃料、ピークと全回収を
照合します。M6 object共有のメモリ会計とも一致が必要です。マーカーだけでは合格しません。
`result.json` の `m7` に生データ・min/median/max・削減量、`m7-benchmark.md` に自動表を保存します。
M8は17dispatchの順序・燃料・結果、実物理ページ共有、失効後の認可拒否、quotaと全回収を
必須にします。`result.json` の `m8` に検証済みの生データを保存します。
M9は外部moduleのfingerprint/予約領域と実行結果・燃料・code回収を照合します。
`verify`は同一ELFからmodule-alt / module-bad-version / module-bad-opcodeのISOを追加生成し、
各128 MiBで起動します。版/命令が不正ならadmitted=0の拒否を成功条件にし、通常M9完了とは区別します。
記録は`result.json`の`m9`。M10では`cargo test --doc`による借用境界検査を追加し、
M10では25コマンドを記録しました。QEMUではM10の4終了ケースと実frame収支を
照合し、`result.json`の`m10`へ保存します。不正M9入力を拒否した後もM10のkernel fixtureは検査します。
M11は`examples/bytecode/shared.json`を2個目のmoduleにします。shared-alt / shared-bad-opcode /
shared-bad-countを追加し、M11のverifyは31コマンドでした。read frame/認可/fuel/回収を
`result.json`の`m11`に保存し、不正opcode/件数では初期権限の準備前に拒否することを照合します。
通常の起動上限は180秒です。M11の初回試験でM6ベンチマーク中に90秒を超えたため余裕を増やしました。
検査項目は省略せず、意図的なtimeout試験は2秒を維持します。180秒で完了しなければ失敗です。
M12は`examples/bytecode/revocation.json`を3個目のmoduleにし、3caseの失効/取消/再起動を
`result.json`の`m12`へ記録します。revocation-bad-opcodeも起動し、現在のverifyは33コマンドです。
M13は同じ3個目のmoduleで、1命令quantumの通常終了と取消+失効を検査します。
20回のsnapshot・80件のtask状態・実read/frame/回収を`result.json`の`m13`へ保存します。
新しいmodule/variantは不要で、verifyは33コマンドのままです。
パニック版はM1途中、想定外fault版はM2途中で意図的に停止するため、M3〜M13を実行しません。
128 MiB・256 MiBが検証対象で、usable領域の終端が1 GiBを超える構成は明示的に拒否します。
各試験は新しい証跡フォルダを使い、過去のシリアルログを流用しません。
QEMUはネットワークなし、ホストの物理ディスク接続なしで実行します。
OVMFコードは読取専用で、可変ファームウェア領域は実行ごとのコピーだけを書き換えます。

## WSL側の個別起動

WindowsでELFを作成した後、WSLから同じリポジトリに移動して実行できます。

```sh
python3 tools/qemu.py image
python3 tools/qemu.py test --memory 128
python3 tools/qemu.py test --memory 256
```

WSL内にRustを別途導入した場合、`cargo build --locked --offline --release
--target x86_64-unknown-none --bin shared-first-kernel` という構成も想定できますが、
LinuxホストでのRustビルドは今回未検証です。

ISOの日時メタデータ等は固定していないため、ビット単位で同じISOになるとは主張しません。
再現可能なコマンド、固定した主要入力、実際に試験したISOのハッシュを提供します。

## 切り分け

- Rustターゲット不足: `rustup target list --installed` で確認します。
- ホスト単体テストのリンク失敗: MSVCのC++ビルドツールを確認します。
- QEMU起動失敗: `artifacts/boot-*/stderr.log` と `result.json` を確認します。
- 起動マーカーなし: 同フォルダの `serial.log` にUEFI/Limineのエラーがないか確認します。
- タイムアウト: QEMUを終了してから失敗を記録します。タイムアウトは起動成功ではありません。
- Clippy・テスト失敗: `artifacts/commands-*/*.log` に各段階の出力が残ります。

物理PCへのインストール、BIOS起動、Secure Boot、GUI、KVMでの挙動は検証範囲外です。
