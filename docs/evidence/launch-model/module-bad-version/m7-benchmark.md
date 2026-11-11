# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1404928 | 366,562.0 | 163,094.0 | 89,392.9 |
| 4 | bytecode | 93 | 1327104 | 60,989.5 | 48,092.5 | 537,277.3 |
| 64 | native | 172 | 1650688 | 1,782,462.5 | 1,574,186.5 | 294,143.1 |
| 64 | bytecode | 153 | 1572864 | 760,892.5 | 739,985.5 | 689,045.6 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
