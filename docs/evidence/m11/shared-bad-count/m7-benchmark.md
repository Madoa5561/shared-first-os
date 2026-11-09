# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1351680 | 384,267.0 | 166,955.5 | 85,274.6 |
| 4 | bytecode | 93 | 1273856 | 66,238.5 | 50,405.0 | 494,722.8 |
| 64 | native | 172 | 1597440 | 1,803,924.5 | 1,609,002.0 | 290,656.9 |
| 64 | bytecode | 153 | 1519616 | 783,398.0 | 754,081.5 | 669,252.6 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
