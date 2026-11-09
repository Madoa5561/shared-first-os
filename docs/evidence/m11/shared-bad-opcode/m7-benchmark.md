# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1351680 | 386,838.0 | 169,342.5 | 84,708.1 |
| 4 | bytecode | 93 | 1273856 | 65,926.0 | 49,100.5 | 497,042.4 |
| 64 | native | 172 | 1597440 | 1,860,006.5 | 1,667,238.5 | 281,927.8 |
| 64 | bytecode | 153 | 1519616 | 807,563.0 | 769,615.0 | 649,234.1 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
