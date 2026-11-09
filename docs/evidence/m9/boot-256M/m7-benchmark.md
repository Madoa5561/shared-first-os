# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 176 | 1597440 | 414,183.5 | 176,748.0 | 79,194.5 |
| 4 | bytecode | 157 | 1519616 | 70,082.0 | 52,790.5 | 467,580.6 |
| 64 | native | 236 | 1843200 | 2,025,207.0 | 1,778,759.0 | 258,936.5 |
| 64 | bytecode | 217 | 1765376 | 863,575.0 | 868,506.0 | 607,117.0 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
