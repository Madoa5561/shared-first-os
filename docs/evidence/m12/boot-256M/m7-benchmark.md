# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 176 | 1630208 | 365,451.0 | 162,835.5 | 89,681.6 |
| 4 | bytecode | 157 | 1552384 | 59,909.0 | 48,718.0 | 546,964.4 |
| 64 | native | 236 | 1875968 | 1,855,819.0 | 1,654,900.5 | 282,511.3 |
| 64 | bytecode | 217 | 1798144 | 783,535.5 | 772,999.5 | 669,193.6 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
