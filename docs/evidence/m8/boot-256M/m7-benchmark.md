# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 176 | 1585152 | 409,069.0 | 179,626.0 | 80,104.4 |
| 4 | bytecode | 157 | 1507328 | 74,075.0 | 56,489.5 | 442,363.4 |
| 64 | native | 236 | 1830912 | 2,046,745.0 | 1,873,583.0 | 256,170.5 |
| 64 | bytecode | 217 | 1753088 | 903,453.0 | 925,634.0 | 580,420.0 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
