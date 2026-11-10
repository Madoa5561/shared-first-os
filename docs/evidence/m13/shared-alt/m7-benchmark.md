# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1380352 | 386,086.5 | 171,462.0 | 84,874.9 |
| 4 | bytecode | 93 | 1302528 | 65,344.5 | 49,827.0 | 501,486.4 |
| 64 | native | 172 | 1626112 | 1,901,913.0 | 1,712,475.0 | 275,667.3 |
| 64 | bytecode | 153 | 1548288 | 797,216.5 | 774,798.0 | 657,713.2 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
