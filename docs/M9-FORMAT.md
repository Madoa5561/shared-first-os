# M9: 外部バイトコードパッケージ

## 境界と合格条件

M8の固定batchに、ISO内の独立ファイルをLimine moduleとして届ける。任意native/ELF/WASMの
アプリローダではない。ブートローダとfile pointer/sizeのメタデータを信頼し、内容バイトは信頼しない。
全プログラムを検証し終えてからBatchを公開し、独自の固定領域へコードをコピーする。
元ファイルが検証後に変更されても、実行するProgramはこの所有コピーだけをborrowする。
Programをschedulerがborrowしている間はBatch.clearをRustの借用規則で禁止する。

Gate: 128/256 MiBの正常起動、同一ELFで外部ファイルの値だけ42→99へ変更する起動、
版番号不正・最後の命令不正でadmitted=0となる起動。全M1〜M8回帰を維持する。
hostテストでは全切断位置、余剰byte、予約bit、各quota、全未知opcode、入力変更後の所有コピーを検査する。

## コンテナ形式 v1

全整数little-endian。固定16-byteヘッダの後、宣言件数だけ「16-byte entry + code」を並べる。
余剰byteは認めない。

| ヘッダoffset | 長さ | 内容 |
| --- | --- | --- |
| 0 | 8 | ASCII `SFOSBC09` |
| 8 | 2 | version=1 |
| 10 | 2 | program count=1..4 |
| 12 | 4 | reserved=0 |

| entry offset | 長さ | 内容 |
| --- | --- | --- |
| 0 | 4 | code length、16の倍数、16..256 |
| 4 | 4 | lifetime fuel、正整数、batch総計2,097,152以下 |
| 8 | 8 | reserved=0 |
| 16 | code length | M7形式の16-byte命令 |

合計codeは512 byte以下、入力全体は592 byte以下。静的storageは4×256 byteを予約し、
実code長と予約容量を区別する。検証前に範囲を確認し、header/codeの読取りや加算を有界にする。
入力にtask ID、権限付与、native entry、物理アドレス、ポインタ再配置の機能はない。
fuel要求は信用せず全体上限を検査する。

## 起動・所有期間・認可

`limine.conf` の `module_path` から `/boot/programs.sfbc` をロードする。M9時点は1個だけ受理。
M11は2個、M12以降は正確に3個を要求する。index 0はM9、index 1の`/boot/shared.sfbc`はM11、
index 2の`/boot/revocation.sfbc`はM12へ渡す。後者2つは信頼されたREAD manifestを利用する。
コンテナ形式と各ファイルの上限は同じで、3moduleとも予約ページを保持する。
fileのsizeをslice生成前に検査し、全範囲がmemory mapのkind=6内にあることを検査する。
M2はkind 6をHHDMへmapしallocatorから除外しており、その範囲を引き続き保持する。
元モジュールの予約ページは回収しない。コード実行用の所有コピーはkernel stack上に置く。
これらを混同して「モジュールの物理RAMを全解放した」とは主張しない。

全件検証後に新規Schedulerへadmitする。初期Capability表は空で、数値handleを知っていても
権限は得られない。読取りはM7のtask-local ObjectStore認可に接続する。
HALT/faultでrelease_task、全terminal後にSchedulerのborrowを終了してBatch.clearする。
これにより実行中のcode破棄を防ぎ、固定code bufferを再利用可能にする。secure erase保証はない。

module contentのFNV-1aはシリアル照合の診断値で、署名/認証/衝突耐性の保証ではない。
host側ではISO・ELF・moduleのSHA256も記録する。ログ出力は最大64dispatchに抑える。
悪意あるbootloader、DMA、SMP、署名、ファイルシステム、動的ロード、ブートローダによる
事前ファイル確保量の制限は対象外。全transport metadataへの悪意あるポインタ耐性は主張しない。

## パッケージ作成

```sh
python3 tools/pack_bytecode.py examples/bytecode/demo.json build/programs.sfbc
```

JSONは`programs`配列、各要素は`fuel`と`instructions`。命令は
`[opcode,dst,src,aux,immediate]`。packerはサイズ/整数/quotaを検査するが命令の意味の最終検証はkernel側。
入力JSONは64 KiBまで読み、出力ファイルが既存なら上書きしない。新しい依存やネットワークは使わない。
通常の`project.ps1 build`はdemo.jsonを自動packしてISOへ入れる。demo.jsonの変更にkernel再コンパイルは不要。
`test-boot`は検証用fixtureの具体的な出力を要求するため、任意に変更したプログラムはQEMUの
手動起動で観測する。汎用アプリ向けCLI/カーネルshellではない。

ABIの根拠: [Limine v8.7.0 header](https://raw.githubusercontent.com/limine-bootloader/limine/v8.7.0/limine.h)、
[protocol](https://raw.githubusercontent.com/limine-bootloader/limine/v8.7.0/PROTOCOL.md)、
[module_path](https://raw.githubusercontent.com/limine-bootloader/limine/v8.7.0/CONFIG.md)。既存THIRD_PARTYの通知を適用する。
