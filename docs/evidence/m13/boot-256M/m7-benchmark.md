# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 176 | 1642496 | 369,013.5 | 163,795.5 | 88,828.1 |
| 4 | bytecode | 157 | 1564672 | 64,075.0 | 48,369.0 | 511,407.6 |
| 64 | native | 236 | 1888256 | 1,904,248.5 | 1,679,645.5 | 275,446.8 |
| 64 | bytecode | 217 | 1810432 | 793,205.0 | 801,797.0 | 661,053.0 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
