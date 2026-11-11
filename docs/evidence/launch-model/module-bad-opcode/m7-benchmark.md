# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1404928 | 363,647.0 | 164,975.5 | 90,111.7 |
| 4 | bytecode | 93 | 1327104 | 62,163.5 | 48,534.5 | 527,127.7 |
| 64 | native | 172 | 1650688 | 1,791,330.5 | 1,595,100.5 | 292,680.8 |
| 64 | bytecode | 153 | 1572864 | 760,361.5 | 740,434.0 | 689,524.7 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
