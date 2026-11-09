# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1335296 | 434,028.5 | 180,738.0 | 75,522.8 |
| 4 | bytecode | 93 | 1257472 | 75,271.5 | 57,024.0 | 435,822.9 |
| 64 | native | 172 | 1581056 | 2,038,231.5 | 1,843,824.5 | 257,226.9 |
| 64 | bytecode | 153 | 1503232 | 872,540.0 | 858,610.5 | 600,897.2 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
