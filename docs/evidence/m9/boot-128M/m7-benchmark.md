# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1335296 | 451,433.0 | 192,130.5 | 72,593.9 |
| 4 | bytecode | 93 | 1257472 | 72,432.0 | 55,721.0 | 452,583.1 |
| 64 | native | 172 | 1581056 | 2,812,575.5 | 2,007,420.0 | 186,480.7 |
| 64 | bytecode | 153 | 1503232 | 926,558.5 | 927,000.0 | 565,851.7 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
