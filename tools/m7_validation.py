"""Restricted bytecode evidence, separate from native CPL3 isolation."""
import re
from m6_validation import records, distribution

ATTACKS = ['unknown_opcode', 'native_opcode', 'object_write', 'bad_register',
           'reserved_bits', 'bad_jump', 'truncated', 'oversized', 'local_read',
           'local_write', 'local_overflow', 'object_bounds', 'object_overflow',
           'foreign_handle', 'forged_generation', 'missing_read', 'malformed_handle',
           'loop_fuel', 'bulk_fuel', 'scratch_isolation', 'revoked_resume', 'stale_reuse']
CONFIG = set('conditions samples warmups runs attacks kernel_image_bytes object_metadata trace_bytes vm_context_bytes native_context_bytes bytecode_bytes file_pages compressed_bytes'.split())
FIELDS = set('pages mode round warmup root0 root1 cpl tables task_frames payload pool_peak before after maps final_tables final_maps sum0 sum1 repeat0 repeat1 fuel read_ticks repeat_ticks shared cleanup'.split())


def expected_order():
    for pages in [4, 64]:
        for sample in range(9):
            for order in range(2):
                yield pages, (sample + order) % 2, sample


def validate_m7(serial, m2, m6):
    serial = serial.replace('\r', '')
    if serial.count('M7 PASS\n') != 1 or serial.count('M7CHECK measured=32 warmups=4 attacks=22 shared=1 fuel=1 revocation=1 native_paging=1 cleanup=1\n') != 1:
        raise ValueError('Missing or duplicate M7 completion')
    configs = records(serial, 'M7CONFIG', CONFIG)
    if len(configs) != 1:
        raise ValueError('Missing or duplicate M7 configuration')
    config = configs[0]
    expected_config = dict(conditions=4, samples=8, warmups=1, runs=36, attacks=22,
                           kernel_image_bytes=m6['config']['kernel_image_bytes'],
                           object_metadata=m6['config']['object_metadata'], trace_bytes=520,
                           vm_context_bytes=272, native_context_bytes=400, bytecode_bytes=192,
                           file_pages=0, compressed_bytes=0)
    if config != expected_config:
        raise ValueError('M7 metadata/configuration mismatch')
    attacks = re.findall(r'^M7ATTACK name=(\w+) denied=1 unchanged=1$', serial, re.M)
    if attacks != ATTACKS or len(re.findall(r'^M7ATTACK ', serial, re.M)) != len(ATTACKS):
        raise ValueError('Missing, repeated or failed M7 attack checks')
    kernel_roots = re.findall(r'^M2PAGING root=(0x[0-9a-f]+) ', serial, re.M)
    if len(kernel_roots) != 1:
        raise ValueError('Missing kernel root evidence')
    kernel_root = int(kernel_roots[0], 0)
    runs = records(serial, 'M7RUN', FIELDS)
    if [(r['pages'], r['mode'], r['round']) for r in runs] != list(expected_order()):
        raise ValueError('Incomplete or reordered M7 matrix')
    tasks = [tuple(int(value, 0) for value in row) for row in re.findall(
        r'^M7TASK task=(\d+) root=(0x[0-9a-f]+) code=(0x[0-9a-f]+) stack=(0x[0-9a-f]+) attack=0$', serial, re.M)]
    if len(tasks) != 36:
        raise ValueError('Missing actual native control tasks')
    native_index = 0
    for run in runs:
        pages, mode = run['pages'], run['mode']
        native = mode == 0
        checksum = 4096 * pages * (pages + 1) // 2
        expected = dict(warmup=int(run['round'] == 0), cpl=3 if native else 0,
                        tables=14 if native else 0, task_frames=5 if native else 0,
                        payload=pages, pool_peak=m2['allocated_frames'] + pages + (19 if native else 0),
                        before=m2['allocated_frames'], after=m2['allocated_frames'],
                        maps=pages * 2 + 5 if native else 0,
                        final_tables=m2['table_frames'], final_maps=m2['virtual_mappings'],
                        sum0=checksum, sum1=checksum, repeat0=checksum, repeat1=checksum,
                        fuel=0 if native else 4 * pages * 4096 + 12, shared=1, cleanup=1)
        if any(run[key] != value for key, value in expected.items()):
            raise ValueError('M7 isolation, accounting, checksum, fuel or cleanup mismatch')
        if run['read_ticks'] <= 0 or run['repeat_ticks'] <= 0:
            raise ValueError('Invalid M7 timings')
        if native:
            pair = tasks[native_index:native_index + 2]
            native_index += 2
            if run['root0'] == run['root1'] or kernel_root in (run['root0'], run['root1']):
                raise ValueError('Native M7 roots are not isolated')
            for task, record in enumerate(pair):
                if (record[0] != task or record[1] != run[f'root{task}'] or len(set(record[1:])) != 3
                        or any(value <= 0 or value % 4096 for value in record[1:])):
                    raise ValueError('Invalid native task evidence')
        elif run['root0'] != kernel_root or run['root1'] != kernel_root:
            raise ValueError('Bytecode did not share kernel address space')
    summary, comparisons = [], []
    for pages in [4, 64]:
        for mode in [0, 1]:
            group = [r for r in runs if (r['pages'], r['mode']) == (pages, mode) and not r['warmup']]
            summary.append(dict(pages=pages, mode='native' if mode == 0 else 'bytecode', samples=8,
                                pool_peak_frames=group[0]['pool_peak'], payload_frames=pages,
                                kernel_image_plus_pool_peak_bytes=config['kernel_image_bytes'] + group[0]['pool_peak'] * 4096,
                                read_ticks=distribution([r['read_ticks'] for r in group]),
                                repeat_ticks=distribution([r['repeat_ticks'] for r in group]),
                                read_bytes_per_million_ticks=distribution([2 * pages * 4096 * 1000000 / r['read_ticks'] for r in group])))
        baseline = [r for r in m6['summary'] if r['pages'] == pages and r['dirty'] == 0 and r['mode'] == 'object-shared-ro']
        if len(baseline) != 1 or baseline[0]['pool_peak_frames'] != summary[-2]['pool_peak_frames']:
            raise ValueError('M7 native memory disagrees with M6 control')
        saved = summary[-2]['pool_peak_frames'] - summary[-1]['pool_peak_frames']
        comparisons.append(dict(pages=pages, pool_frames_saved=saved, pool_bytes_saved=saved * 4096,
                                m6_native_pool_peak=baseline[0]['pool_peak_frames']))
    return dict(config=config, attacks=attacks, runs=runs, summary=summary, comparisons=comparisons,
                timing_unit='QEMU guest TSC ticks; different execution mechanisms, not general application speedups',
                scope='read-only restricted bytecode; native isolation remains hardware paging')


def benchmark_markdown(result):
    lines = ['# M7 自動集計', '', '各条件8サンプル、別にwarmup 1回を保存。時間はゲストTSC tick。',
             'bytecodeは制限付き言語をring 0の信頼されたインタプリタで実行。nativeは独立CR3・CPL3。',
             'SUMは検査付きhost処理であり、時間比は一般言語・任意アプリの速度比ではありません。', '',
             '| pages | mode | pool peak | kernel image + pool bytes | read median tick | repeat median tick | read bytes / 10^6 tick |',
             '| ---: | --- | ---: | ---: | ---: | ---: | ---: |']
    for row in result['summary']:
        lines.append(f"| {row['pages']} | {row['mode']} | {row['pool_peak_frames']} | {row['kernel_image_plus_pool_peak_bytes']} | {row['read_ticks']['median']:,.1f} | {row['repeat_ticks']['median']:,.1f} | {row['read_bytes_per_million_ticks']['median']:,.1f} |")
    lines.extend(['', 'min/median/maxと全36試行・22攻撃ケースはresult.jsonのm7を参照。',
                  '同一バイナリ内の対照群比較。全管理配列・trace・runtimeコードは画像に含む。',
                  'ブートローダ・ファームウェア等を含むマシン全体のRAM量ではありません。', ''])
    return '\n'.join(lines)
