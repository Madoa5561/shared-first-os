"""Strict finite M5 A/B protocol validation; no timing speedup is assumed."""
import re
import statistics


def workload_data(workload, obj, offset):
    if workload == 0:
        return 0x5a
    if workload == 1:
        return obj * 4 + offset // 4096 + 1
    mask = (1 << 64) - 1
    value = (((obj << 32) | offset) + 0x9e3779b97f4a7c15) & mask
    value = ((value ^ (value >> 30)) * 0xbf58476d1ce4e5b9) & mask
    value = ((value ^ (value >> 27)) * 0x94d049bb133111eb) & mask
    return (value ^ (value >> 31)) & 255


def input_metrics(workload):
    digest = 0xcbf29ce484222325
    sums = [0] * 4
    for obj in range(4):
        for offset in range(16384):
            byte = workload_data(workload, obj, offset)
            sums[obj] += byte
            digest = ((digest ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
    return digest, sums


def validate_m5(serial, m2):
    serial = serial.replace('\r', '')
    check = 'M5CHECK workloads=3 samples=3 runs=18 ring3=1 independent_revoke=1 pinned=1 cow=1 cleanup=1\n'
    if serial.count('M5 PASS\n') != 1 or serial.count(check) != 1:
        raise ValueError('Missing or duplicate M5 completion evidence')
    lines = re.findall(r'^M5RUN (.+)$', serial, re.M)
    if len(lines) != 18:
        raise ValueError('M5 requires all 18 A/B samples')
    fields = set('workload sample enabled input_hash logical_pages created cold compressed_pages compressed_bytes mapped mappings cow hits probes bypasses attempts rejected restored metadata arena object_metadata free_before watermark before after mapped_allocated extra_tables task_frames final_tables final_mappings create_ticks pressure_ticks map_read_ticks cow_read_ticks cleanup_ticks isolation reclaimed'.split())
    runs = []
    for line in lines:
        pairs = [field.split('=') for field in line.split()]
        if len(pairs) != len(fields) or any(len(pair) != 2 for pair in pairs) or {pair[0] for pair in pairs} != fields:
            raise ValueError('Malformed M5 sample')
        runs.append({key: int(value, 0) for key, value in pairs})
    expected_order = [(workload, sample, enabled) for workload in range(3) for sample in range(3)
                      for enabled in ([0, 1] if sample % 2 == 0 else [1, 0])]
    if [(r['workload'], r['sample'], r['enabled']) for r in runs] != expected_order:
        raise ValueError('Unpaired or reordered M5 workloads')
    metrics = [input_metrics(workload) for workload in range(3)]
    tasks = [tuple(int(value, 0) for value in row) for row in re.findall(
        r'^M5TASK task=(\d+) root=(0x[0-9a-f]+) code=(0x[0-9a-f]+) stack=(0x[0-9a-f]+) attack=0$', serial, re.M)]
    users = [tuple(int(value, 0) for value in row) for row in re.findall(
        r'^M5USER task=(\d+) phase=(\d+) checksum=(\d+) cs=3 root=(0x[0-9a-f]+)$', serial, re.M)]
    if len(tasks) != 36 or len(users) != 90:
        raise ValueError('Missing M5 real-user checkpoints')
    for index, run in enumerate(runs):
        workload, enabled = run['workload'], run['enabled']
        expected = dict(created=16, cold=16, compressed_pages=0, compressed_bytes=0,
                        mapped=16, cow=17, hits=0, probes=0, bypasses=0, attempts=0, rejected=0, restored=0)
        if enabled:
            if workload == 0:
                expected.update(created=1, cold=0, compressed_pages=1, compressed_bytes=3,
                                mapped=1, cow=2, hits=15, probes=15, attempts=1, restored=1)
            elif workload == 1:
                expected.update(cold=0, compressed_pages=16, compressed_bytes=48,
                                mapped=8, cow=9, bypasses=8, attempts=16, restored=8)
            else:
                expected.update(bypasses=8, attempts=16, rejected=16)
        expected.update(logical_pages=16, mappings=8, isolation=1, reclaimed=1,
                        before=m2['allocated_frames'], after=m2['allocated_frames'],
                        input_hash=metrics[workload][0], arena=4096, task_frames=5,
                        extra_tables=14, final_tables=m2['table_frames'], final_mappings=m2['virtual_mappings'])
        if any(run[key] != value for key, value in expected.items()):
            raise ValueError('M5 data/accounting/policy mismatch')
        if (run['metadata'] <= 0 or run['object_metadata'] <= 0
                or run['mapped_allocated'] != run['before'] + run['mapped'] + run['extra_tables'] + run['task_frames']
                or run['free_before'] != m2['free_frames'] - run['created']
                or run['watermark'] != run['free_before'] + 32
                or any(run[key] <= 0 for key in fields if key.endswith('_ticks'))):
            raise ValueError('Invalid M5 metadata, pressure threshold or raw timing')
        roots = []
        for task, row in enumerate(tasks[index * 2:index * 2 + 2]):
            if row[0] != task or len(set(row[1:])) != 3 or any(value <= 0 or value % 4096 for value in row[1:]):
                raise ValueError('Invalid M5 private task pages')
            roots.append(row[1])
        if roots[0] == roots[1]:
            raise ValueError('M5 task roots alias')
        sums = metrics[workload][1]
        byte = workload_data(workload, 0, 0)
        expected_users = [(0, 0, sums[0], roots[0]), (1, 0, sums[1], roots[1]),
                          (0, 1, sums[0] - byte + (byte ^ 255), roots[0]),
                          (1, 1, sums[1], roots[1]), (1, 2, sums[1], roots[1])]
        if users[index * 5:index * 5 + 5] != expected_users:
            raise ValueError('M5 reader changed or actual CPL3/CR3 evidence missing')
    for marker in ['M5COW task=0 address=0x40000000 error=7 cs=3 copies=1',
                   'M5FAULT task=0 vector=14 error=4 address=0x40000000 cs=3 killed=1',
                   'M5EXIT task=1 status=0 cs=3']:
        if serial.count(marker + '\n') != 18:
            raise ValueError('M5 COW/revoke/exit evidence incomplete')
    if 'M5DENY' in serial or 'M5COWFAIL' in serial:
        raise ValueError('Unexpected M5 syscall/COW failure')
    if any(len(re.findall(r'^' + prefix + r' ', serial, re.M)) != 18 for prefix in ['M5COW', 'M5FAULT', 'M5EXIT']):
        raise ValueError('Unexpected extra M5 exception or exit')
    for field in ['metadata', 'object_metadata', 'arena']:
        if len({run[field] for run in runs}) != 1:
            raise ValueError('M5 A/B reserved storage differs')
    summary = []
    for workload in range(3):
        for enabled in range(2):
            group = [run for run in runs if run['workload'] == workload and run['enabled'] == enabled]
            summary.append({'workload': workload, 'enabled': bool(enabled), 'samples': 3,
                            'cold_accounted_bytes': group[0]['cold'] * 4096 + group[0]['metadata'] + group[0]['arena'] + group[0]['object_metadata'],
                            'timing_ticks': {key: {'min': min(r[key] for r in group),
                                                   'median': statistics.median(r[key] for r in group),
                                                   'max': max(r[key] for r in group)}
                                             for key in sorted(fields) if key.endswith('_ticks')}})
    return {'runs': runs, 'summary': summary, 'timing_unit': 'QEMU guest TSC ticks; not hardware cycles or calibrated time',
            'compression_pressure': 'controlled free-frame watermark; not naturally exhausted memory',
            'reserved_storage_included': True, 'final_allocated_frames': m2['allocated_frames']}
