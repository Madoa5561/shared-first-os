# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1339392 | 404,541.5 | 173,229.5 | 81,001.4 |
| 4 | bytecode | 93 | 1261568 | 69,141.5 | 50,001.5 | 473,935.8 |
| 64 | native | 172 | 1585152 | 2,050,772.5 | 1,869,833.5 | 255,715.1 |
| 64 | bytecode | 153 | 1507328 | 841,336.0 | 817,882.5 | 623,161.8 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
