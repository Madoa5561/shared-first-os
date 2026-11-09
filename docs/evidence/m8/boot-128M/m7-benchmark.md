# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1323008 | 451,548.0 | 186,094.0 | 72,634.1 |
| 4 | bytecode | 93 | 1245184 | 72,184.5 | 54,935.5 | 453,948.2 |
| 64 | native | 172 | 1568768 | 2,131,193.0 | 1,917,912.5 | 246,009.7 |
| 64 | bytecode | 153 | 1490944 | 919,442.5 | 881,683.5 | 570,239.2 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
