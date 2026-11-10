# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1380352 | 384,765.0 | 164,717.0 | 85,163.8 |
| 4 | bytecode | 93 | 1302528 | 66,002.0 | 48,850.0 | 496,474.5 |
| 64 | native | 172 | 1626112 | 1,820,341.5 | 1,652,962.5 | 288,020.5 |
| 64 | bytecode | 153 | 1548288 | 809,020.5 | 767,998.5 | 648,052.9 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
