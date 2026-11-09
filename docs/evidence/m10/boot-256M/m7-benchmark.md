# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 176 | 1601536 | 425,368.0 | 188,763.5 | 77,116.7 |
| 4 | bytecode | 157 | 1523712 | 69,874.5 | 53,109.5 | 468,955.4 |
| 64 | native | 236 | 1847296 | 2,007,955.5 | 1,917,641.0 | 261,114.4 |
| 64 | bytecode | 217 | 1769472 | 883,631.0 | 864,297.0 | 593,391.8 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
