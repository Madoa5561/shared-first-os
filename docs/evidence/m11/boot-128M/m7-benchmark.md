# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1351680 | 369,123.0 | 166,434.5 | 88,777.0 |
| 4 | bytecode | 93 | 1273856 | 66,709.5 | 50,409.5 | 491,205.5 |
| 64 | native | 172 | 1597440 | 1,929,352.0 | 1,690,732.0 | 271,746.5 |
| 64 | bytecode | 153 | 1519616 | 818,554.0 | 803,084.5 | 640,580.7 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
