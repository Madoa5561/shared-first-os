"""M6 exact protocol, accounting and descriptive statistics. No speedup gate."""
import re
import statistics

MODES = ['eager-private', 'raw-shared-ro', 'raw-cow', 'object-shared-ro', 'object-cow']
TIMINGS = ['create_ticks', 'map_ticks', 'read_ticks', 'write_read_ticks',
           'handler_ticks', 'handler_max', 'unmap_ticks', 'release_ticks']
FIELDS = set(('pages dirty mode round order warmup root0 root1 cs0 cs1 input_sum writer_sum reader_sum '
              'payload_peak pool_peak before after extra_tables task_frames mapping_pages final_tables '
              'final_mappings metadata faults copies calls isolation cleanup').split() + TIMINGS)
CONFIG = set('samples warmups sizes conditions runs kernel_image_bytes raw_metadata object_metadata reserved_bench_bytes task_context_bytes fence_min file_pages compressed_bytes timed_serial'.split())


def records(serial, prefix, fields):
    result = []
    for line in re.findall(r'^' + prefix + r' (.+)$', serial, re.M):
        pairs = [part.split('=') for part in line.split()]
        if len(pairs) != len(fields) or any(len(pair) != 2 for pair in pairs) or {pair[0] for pair in pairs} != fields:
            raise ValueError(f'Malformed {prefix} fields')
        result.append({key: int(value, 0) for key, value in pairs})
    return result


def expected_order():
    for pages in [4, 64]:
        for dirty in [0, 1, pages]:
            modes = [0, 1, 2, 3, 4] if dirty == 0 else [0, 2, 4]
            for sample in range(9):
                for order in range(len(modes)):
                    yield pages, dirty, modes[(sample + order) % len(modes)], sample, order


def distribution(values):
    return {'min': min(values), 'median': statistics.median(values), 'max': max(values)}


def validate_m6(serial, m2):
    serial = serial.replace('\r', '')
    if serial.count('M6 PASS\n') != 1 or serial.count('M6CHECK measured=176 warmups=22 peak_tracking=1 checksum=1 physical_identity=1 isolation=1 cleanup=1\n') != 1:
        raise ValueError('Missing or duplicate M6 completion evidence')
    configs = records(serial, 'M6CONFIG', CONFIG)
    if len(configs) != 1:
        raise ValueError('Missing or duplicate M6 configuration')
    config = configs[0]
    for key, value in dict(samples=8, warmups=1, sizes=2, conditions=22, runs=198,
                           raw_metadata=1024, task_context_bytes=400, file_pages=0,
                           compressed_bytes=0, timed_serial=0).items():
        if config[key] != value:
            raise ValueError('Wrong M6 configuration')
    if (config['kernel_image_bytes'] % 4096 or config['kernel_image_bytes'] <= config['reserved_bench_bytes']
            or config['object_metadata'] <= config['raw_metadata'] or config['fence_min'] <= 0
            or config['reserved_bench_bytes'] != config['raw_metadata'] + config['object_metadata']):
        raise ValueError('Invalid M6 memory/timer configuration')
    runs = records(serial, 'M6RUN', FIELDS)
    expected = list(expected_order())
    if len(runs) != 198 or [(r['pages'], r['dirty'], r['mode'], r['round'], r['order']) for r in runs] != expected:
        raise ValueError('Missing, repeated or unpaired M6 trials')
    tasks = [tuple(int(value, 0) for value in row) for row in re.findall(
        r'^M6TASK task=(\d+) root=(0x[0-9a-f]+) code=(0x[0-9a-f]+) stack=(0x[0-9a-f]+) attack=0$', serial, re.M)]
    if len(tasks) != 396:
        raise ValueError('M6 requires two actual task launches per trial')
    for index, run in enumerate(runs):
        pages, dirty, mode = run['pages'], run['dirty'], run['mode']
        faults = dirty if mode in [2, 4] else 0
        payload = pages * 2 if mode == 0 else pages + faults
        total = 4096 * pages * (pages + 1) // 2
        expected_values = dict(warmup=int(run['round'] == 0), cs0=3, cs1=3,
                               input_sum=total, reader_sum=total,
                               writer_sum=total + dirty * 255 - dirty * (dirty + 1),
                               payload_peak=payload, pool_peak=m2['allocated_frames'] + 19 + payload,
                               before=m2['allocated_frames'], after=m2['allocated_frames'],
                               extra_tables=14, task_frames=5, mapping_pages=pages * 2,
                               final_tables=m2['table_frames'], final_mappings=m2['virtual_mappings'],
                               metadata=config['object_metadata' if mode >= 3 else 'raw_metadata'],
                               faults=faults, copies=pages if mode == 0 else faults, calls=6,
                               isolation=1, cleanup=1)
        if any(run[key] != value for key, value in expected_values.items()):
            raise ValueError('M6 checksum, memory, copy/fault or cleanup mismatch')
        if any(run[key] <= 0 for key in TIMINGS if not key.startswith('handler_')):
            raise ValueError('Non-positive M6 elapsed ticks')
        if ((faults == 0 and (run['handler_ticks'] != 0 or run['handler_max'] != 0))
                or (faults > 0 and not 0 < run['handler_max'] <= run['handler_ticks'] <= run['handler_max'] * faults)):
            raise ValueError('M6 handler timing/count mismatch')
        if run['root0'] == run['root1']:
            raise ValueError('M6 address-space roots alias')
        for task, row in enumerate(tasks[index * 2:index * 2 + 2]):
            if (row[0] != task or row[1] != run[f'root{task}'] or len(set(row[1:])) != 3
                    or any(value <= 0 or value % 4096 for value in row[1:])):
                raise ValueError('Invalid M6 real task/CPL3 evidence')
    summary = []
    for pages in [4, 64]:
        for dirty in [0, 1, pages]:
            for mode in ([0, 1, 2, 3, 4] if dirty == 0 else [0, 2, 4]):
                group = [r for r in runs if (r['pages'], r['dirty'], r['mode']) == (pages, dirty, mode) and not r['warmup']]
                summary.append({'pages': pages, 'dirty': dirty, 'mode': MODES[mode], 'samples': len(group),
                                'payload_peak_frames': group[0]['payload_peak'],
                                'pool_peak_frames': group[0]['pool_peak'],
                                'kernel_image_plus_pool_peak_bytes': config['kernel_image_bytes'] + group[0]['pool_peak'] * 4096,
                                'selected_metadata_bytes': group[0]['metadata'],
                                'faults': group[0]['faults'], 'copies': group[0]['copies'],
                                'timing_ticks': {key: distribution([r[key] for r in group]) for key in TIMINGS},
                                'read_bytes_per_million_ticks': distribution([pages * 4096 * 2 * 1_000_000 / r['read_ticks'] for r in group]),
                                'handler_mean_ticks': distribution([r['handler_ticks'] / r['faults'] for r in group]) if dirty and mode in [2, 4] else None})
    return {'config': config, 'runs': runs, 'summary': summary,
            'timing_unit': 'QEMU guest TSC ticks; not hardware cycles or calibrated seconds',
            'warmups_excluded_from_summary': 22, 'final_allocated_frames': m2['allocated_frames']}


def benchmark_markdown(result):
    lines = ['# M6 自動集計（当該QEMU起動）', '',
             '各条件8サンプル。条件ごとの最初の1回は保存した上で集計から除外。時間はゲストTSC tick。',
             '診断出力は計時区間外。kernel APIのバッチ時間であり、汎用syscallの単発遅延や実機性能ではありません。', '',
             '| pages | dirty | mode | payload peak | copies / faults | create中央値 | map中央値 | read中央値 | write/read中央値 | read bytes / 10^6 tick中央値 |',
             '| ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |']
    for row in result['summary']:
        times = row['timing_ticks']
        lines.append(f"| {row['pages']} | {row['dirty']} | {row['mode']} | {row['payload_peak_frames']} | {row['copies']} / {row['faults']} | {times['create_ticks']['median']:,.0f} | {times['map_ticks']['median']:,.0f} | {times['read_ticks']['median']:,.0f} | {times['write_read_ticks']['median']:,.0f} | {row['read_bytes_per_million_ticks']['median']:,.0f} |")
    lines.extend(['', 'min/median/max、COW handler、unmap、release、全生サンプルは隣のresult.jsonのm6に保存。',
                  'kernel_image_plus_pool_peak_bytesはカーネル画像とallocatorプールだけの範囲。予約済み管理領域を再加算しません。',
                  'ブートローダ保持領域、ファームウェア、MMIO等を含むマシン全体のRAM総量ではありません。', ''])
    return '\n'.join(lines)
