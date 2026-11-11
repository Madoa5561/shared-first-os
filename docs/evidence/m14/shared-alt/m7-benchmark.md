# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1396736 | 378,802.5 | 163,184.0 | 86,507.4 |
| 4 | bytecode | 93 | 1318912 | 65,798.0 | 49,212.0 | 498,012.9 |
| 64 | native | 172 | 1642496 | 1,838,285.0 | 1,602,820.0 | 285,214.9 |
| 64 | bytecode | 153 | 1564672 | 759,884.5 | 772,346.5 | 689,958.5 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
