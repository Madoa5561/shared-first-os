# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1404928 | 359,356.5 | 157,887.5 | 91,187.3 |
| 4 | bytecode | 93 | 1327104 | 60,559.0 | 47,071.5 | 541,126.3 |
| 64 | native | 172 | 1650688 | 1,773,655.5 | 1,604,297.0 | 295,632.9 |
| 64 | bytecode | 153 | 1572864 | 762,069.5 | 754,560.5 | 687,979.3 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
