# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1380352 | 352,746.5 | 159,040.5 | 92,895.5 |
| 4 | bytecode | 93 | 1302528 | 63,215.0 | 47,802.5 | 518,388.5 |
| 64 | native | 172 | 1626112 | 1,898,619.0 | 1,665,285.0 | 276,149.5 |
| 64 | bytecode | 153 | 1548288 | 793,878.5 | 765,421.5 | 660,433.5 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
