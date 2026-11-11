# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 176 | 1667072 | 366,068.0 | 163,644.0 | 89,522.9 |
| 4 | bytecode | 157 | 1589248 | 63,216.0 | 48,436.5 | 518,541.3 |
| 64 | native | 236 | 1912832 | 1,782,685.0 | 1,557,985.0 | 294,102.3 |
| 64 | bytecode | 217 | 1835008 | 794,272.5 | 743,677.0 | 660,101.9 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
