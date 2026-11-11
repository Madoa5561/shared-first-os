# M7 自動集計

各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。
bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。
SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。

| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 4 | native | 176 | 1658880 | 375,229.0 | 169,214.5 | 87,384.1 |
| 4 | bytecode | 157 | 1581056 | 63,325.5 | 48,949.0 | 517,492.7 |
| 64 | native | 236 | 1904640 | 1,863,558.0 | 1,669,783.0 | 281,339.1 |
| 64 | bytecode | 217 | 1826816 | 801,009.5 | 786,305.0 | 654,579.5 |

min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。
同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。
ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。
