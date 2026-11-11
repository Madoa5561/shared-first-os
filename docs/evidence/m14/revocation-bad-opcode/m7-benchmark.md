# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1396736 | 377,813.0 | 166,304.0 | 86,735.5 |
| 4 | bytecode | 93 | 1318912 | 65,256.5 | 49,254.5 | 502,148.5 |
| 64 | native | 172 | 1642496 | 1,883,767.5 | 1,666,910.5 | 278,335.4 |
| 64 | bytecode | 153 | 1564672 | 786,106.5 | 766,646.5 | 666,948.9 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
