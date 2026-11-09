# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 176 | 1613824 | 369,824.0 | 170,149.0 | 88,605.7 |
| 4 | bytecode | 157 | 1536000 | 64,212.0 | 50,813.5 | 510,310.4 |
| 64 | native | 236 | 1859584 | 1,853,278.0 | 1,671,849.5 | 282,900.1 |
| 64 | bytecode | 217 | 1781760 | 802,621.5 | 784,033.5 | 653,220.2 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
