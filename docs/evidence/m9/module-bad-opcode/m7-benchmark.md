# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1335296 | 410,829.5 | 183,696.0 | 79,766.6 |
| 4 | bytecode | 93 | 1257472 | 68,925.0 | 53,979.0 | 475,446.7 |
| 64 | native | 172 | 1581056 | 2,017,322.0 | 1,804,059.0 | 259,975.5 |
| 64 | bytecode | 153 | 1503232 | 877,969.5 | 854,703.0 | 597,419.5 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
