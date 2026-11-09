# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 112 | 1335296 | 487,435.5 | 196,300.5 | 68,248.3 |
| 4 | bytecode | 93 | 1257472 | 104,280.0 | 83,791.5 | 338,354.0 |
| 64 | native | 172 | 1581056 | 2,210,182.5 | 1,998,832.5 | 237,412.6 |
| 64 | bytecode | 153 | 1503232 | 899,742.0 | 920,525.0 | 582,716.8 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
