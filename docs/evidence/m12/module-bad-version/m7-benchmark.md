# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1368064 | 375,512.5 | 161,342.0 | 87,262.5 |
| 4 | bytecode | 93 | 1290240 | 63,280.5 | 48,814.0 | 517,835.3 |
| 64 | native | 172 | 1613824 | 1,790,939.5 | 1,620,323.0 | 292,747.0 |
| 64 | bytecode | 153 | 1536000 | 783,497.0 | 758,651.0 | 669,164.1 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
