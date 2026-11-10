# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1368064 | 363,257.0 | 160,085.5 | 90,207.2 |
| 4 | bytecode | 93 | 1290240 | 62,522.5 | 47,722.5 | 524,115.6 |
| 64 | native | 172 | 1613824 | 1,834,721.5 | 1,675,654.5 | 285,876.5 |
| 64 | bytecode | 153 | 1536000 | 774,389.0 | 775,968.5 | 677,075.2 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
