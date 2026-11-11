# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1404928 | 372,987.5 | 164,639.0 | 87,854.8 |
| 4 | bytecode | 93 | 1327104 | 62,781.5 | 48,619.5 | 521,937.9 |
| 64 | native | 172 | 1650688 | 1,809,679.0 | 1,645,332.5 | 289,837.6 |
| 64 | bytecode | 153 | 1572864 | 767,588.5 | 746,522.5 | 683,085.9 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
