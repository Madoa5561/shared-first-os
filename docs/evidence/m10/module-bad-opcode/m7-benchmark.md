# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1339392 | 440,820.0 | 187,213.0 | 74,340.5 |
| 4 | bytecode | 93 | 1261568 | 77,428.5 | 53,917.5 | 423,203.4 |
| 64 | native | 172 | 1585152 | 2,088,531.0 | 1,779,077.0 | 251,071.0 |
| 64 | bytecode | 153 | 1507328 | 870,430.0 | 857,500.0 | 602,335.1 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
