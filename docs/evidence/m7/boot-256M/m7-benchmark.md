# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 176 | 1560576 | 408,320.5 | 177,425.0 | 80,270.1 |
| 4 | bytecode | 157 | 1482752 | 70,459.0 | 56,697.0 | 467,566.6 |
| 64 | native | 236 | 1806336 | 1,967,295.5 | 1,787,279.5 | 266,504.8 |
| 64 | bytecode | 217 | 1728512 | 940,560.5 | 894,898.0 | 557,500.1 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
