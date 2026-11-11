# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1404928 | 382,279.5 | 170,985.0 | 85,720.0 |
| 4 | bytecode | 93 | 1327104 | 61,382.5 | 48,186.5 | 533,843.1 |
| 64 | native | 172 | 1650688 | 1,835,442.0 | 1,633,308.5 | 285,711.4 |
| 64 | bytecode | 153 | 1572864 | 774,973.0 | 753,575.0 | 676,524.2 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
