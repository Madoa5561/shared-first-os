"""Synthetic validator tests, deliberately separate from actual QEMU evidence."""
import unittest
from m5_validation import input_metrics, validate_m5, workload_data


def fixture():
    lines = []
    for workload in range(3):
        digest, sums = input_metrics(workload)
        for sample in range(3):
            for enabled in ([0, 1] if sample % 2 == 0 else [1, 0]):
                for task, root in enumerate([0x1000, 0x5000]):
                    lines.append(f'M5TASK task={task} root={root:#x} code={root + 4096:#x} stack={root + 8192:#x} attack=0')
                byte = workload_data(workload, 0, 0)
                for task, phase, checksum in [(0, 0, sums[0]), (1, 0, sums[1]),
                                              (0, 1, sums[0] - byte + (byte ^ 255)),
                                              (1, 1, sums[1]), (1, 2, sums[1])]:
                    lines.append(f'M5USER task={task} phase={phase} checksum={checksum} cs=3 root={0x1000 + task * 0x4000:#x}')
                lines.extend(['M5COW task=0 address=0x40000000 error=7 cs=3 copies=1',
                              'M5FAULT task=0 vector=14 error=4 address=0x40000000 cs=3 killed=1',
                              'M5EXIT task=1 status=0 cs=3'])
                run = dict(workload=workload, sample=sample, enabled=enabled, input_hash=digest,
                           logical_pages=16, created=16, cold=16, compressed_pages=0, compressed_bytes=0,
                           mapped=16, mappings=8, cow=17, hits=0, probes=0, bypasses=0, attempts=0,
                           rejected=0, restored=0, metadata=4000, arena=4096, object_metadata=5000,
                           free_before=984, watermark=1016, before=89, after=89,
                           mapped_allocated=124, extra_tables=14, task_frames=5, final_tables=73, final_mappings=30000,
                           create_ticks=100, pressure_ticks=20, map_read_ticks=200,
                           cow_read_ticks=150, cleanup_ticks=100, isolation=1, reclaimed=1)
                if enabled:
                    if workload == 0:
                        run.update(created=1, cold=0, compressed_pages=1, compressed_bytes=3,
                                   mapped=1, cow=2, hits=15, probes=15, attempts=1, restored=1,
                                   free_before=999, watermark=1031)
                    elif workload == 1:
                        run.update(cold=0, compressed_pages=16, compressed_bytes=48,
                                   mapped=8, cow=9, bypasses=8, attempts=16, restored=8)
                    else:
                        run.update(bypasses=8, attempts=16, rejected=16)
                run['mapped_allocated'] = 89 + run['mapped'] + 14 + 5
                lines.append('M5RUN ' + ' '.join(f'{key}={value}' for key, value in run.items()))
    return '\n'.join(lines) + '\nM5CHECK workloads=3 samples=3 runs=18 ring3=1 independent_revoke=1 pinned=1 cow=1 cleanup=1\nM5 PASS\n'


class M5ValidatorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.serial = fixture()
        cls.m2 = {'allocated_frames': 89, 'free_frames': 1000, 'table_frames': 73, 'virtual_mappings': 30000}

    def test_accepts_full_ab_protocol_and_retains_raw_samples(self):
        result = validate_m5(self.serial, self.m2)
        self.assertEqual(len(result['runs']), 18)
        self.assertEqual(len(result['summary']), 6)
        self.assertEqual(result['summary'][1]['cold_accounted_bytes'], 13096)

    def test_previous_milestone_or_marker_is_not_success(self):
        for serial in ['M4 PASS\n', 'M5 PASS\n', self.serial.replace('M5 PASS\n', '')]:
            with self.assertRaises(ValueError):
                validate_m5(serial, self.m2)

    def test_rejects_unpaired_inputs_or_missing_runs(self):
        for old, new in [('workload=0 sample=0 enabled=1', 'workload=1 sample=0 enabled=1'),
                         ('input_hash=', 'wrong_hash='), ('M5RUN ', 'SKIPPED '),
                         ('sample=2 enabled=1', 'sample=1 enabled=1')]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                validate_m5(self.serial.replace(old, new, 1), self.m2)

    def test_rejects_faked_memory_savings_or_missing_overheads(self):
        for old, new in [('compressed_bytes=3', 'compressed_bytes=0'), ('after=89', 'after=90'),
                         ('arena=4096', 'arena=0'), ('metadata=4000', 'metadata=0'),
                         ('cow=2 ', 'cow=1 '), ('mapped=8 ', 'mapped=0 '),
                         ('rejected=16', 'rejected=0'), ('bypasses=8', 'bypasses=0'),
                         ('extra_tables=14', 'extra_tables=0'), ('final_tables=73', 'final_tables=74')]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                validate_m5(self.serial.replace(old, new, 1), self.m2)

    def test_rejects_invalid_timing_or_pressure(self):
        for old, new in [('create_ticks=100', 'create_ticks=0'), ('watermark=1016', 'watermark=2'),
                         ('free_before=984', 'free_before=0')]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                validate_m5(self.serial.replace(old, new, 1), self.m2)

    def test_rejects_simulated_user_or_missing_cow_revoke_exit(self):
        for old, new in [('cs=3', 'cs=0'), ('root=0x5000', 'root=0x1000'),
                         ('checksum=1474560', 'checksum=1474561'),
                         ('M5COW task=', 'SKIPPED task='), ('M5FAULT task=', 'SKIPPED task='),
                         ('M5EXIT task=', 'SKIPPED task=')]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                validate_m5(self.serial.replace(old, new, 1), self.m2)
