# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1339392 | 432,579.5 | 187,793.5 | 75,778.1 |
| 4 | bytecode | 93 | 1261568 | 75,575.0 | 54,115.0 | 433,588.1 |
| 64 | native | 172 | 1585152 | 2,114,713.0 | 1,898,446.5 | 247,945.3 |
| 64 | bytecode | 153 | 1507328 | 908,245.0 | 902,259.5 | 577,588.9 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
