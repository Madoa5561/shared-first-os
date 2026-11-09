"""Synthetic validator tests are not a substitute for QEMU execution."""
import copy
import unittest
from m7_validation import ATTACKS, expected_order, validate_m7, benchmark_markdown


def fixture():
    lines = ['M2PAGING root=0x1000 table_frames=73 mappings=30000 wp=1 nx=1',
             'M7CONFIG conditions=4 samples=8 warmups=1 runs=36 attacks=22 kernel_image_bytes=1048576 object_metadata=24384 trace_bytes=520 vm_context_bytes=272 native_context_bytes=400 bytecode_bytes=192 file_pages=0 compressed_bytes=0']
    lines.extend(f'M7ATTACK name={name} denied=1 unchanged=1' for name in ATTACKS)
    for pages, mode, sample in expected_order():
        native = mode == 0
        if native:
            lines.extend(['M7TASK task=0 root=0xa000 code=0xb000 stack=0xc000 attack=0',
                          'M7TASK task=1 root=0xe000 code=0xf000 stack=0x10000 attack=0'])
        checksum = 4096 * pages * (pages + 1) // 2
        row = dict(pages=pages, mode=mode, round=sample, warmup=int(sample == 0),
                   root0=40960 if native else 4096, root1=57344 if native else 4096,
                   cpl=3 if native else 0, tables=14 if native else 0,
                   task_frames=5 if native else 0, payload=pages,
                   pool_peak=89 + pages + (19 if native else 0), before=89, after=89,
                   maps=2 * pages + 5 if native else 0, final_tables=73, final_maps=30000,
                   sum0=checksum, sum1=checksum, repeat0=checksum, repeat1=checksum,
                   fuel=0 if native else 4 * pages * 4096 + 12,
                   read_ticks=10000 if sample == 0 else 100 + sample,
                   repeat_ticks=200 + sample, shared=1, cleanup=1)
        lines.append('M7RUN ' + ' '.join(f'{key}={value}' for key, value in row.items()))
    lines.extend(['M7CHECK measured=32 warmups=4 attacks=22 shared=1 fuel=1 revocation=1 native_paging=1 cleanup=1', 'M7 PASS', ''])
    return '\n'.join(lines)


class M7ValidatorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.serial = fixture()
        cls.m2 = dict(allocated_frames=89, table_frames=73, virtual_mappings=30000)
        cls.m6 = dict(config=dict(kernel_image_bytes=1048576, object_metadata=24384),
                      summary=[dict(pages=pages, dirty=0, mode='object-shared-ro', pool_peak_frames=89 + 19 + pages) for pages in [4, 64]])

    def test_valid_comparison_and_warmup_exclusion(self):
        result = validate_m7(self.serial, self.m2, self.m6)
        self.assertEqual(len(result['runs']), 36)
        self.assertEqual(len(result['attacks']), 22)
        for group in result['summary']:
            self.assertEqual(group['read_ticks'], dict(min=101, median=104.5, max=108))
        self.assertEqual([r['pool_bytes_saved'] for r in result['comparisons']], [77824, 77824])
        self.assertIn('bytecode', benchmark_markdown(result))

    def test_rejects_missing_attacks_and_matrix(self):
        for serial in ['M7 PASS\n', self.serial + self.serial,
                       self.serial.replace('M7RUN ', 'OMITTED ', 1),
                       self.serial.replace('M7ATTACK name=revoked_resume', 'OMITTED name=revoked_resume'),
                       self.serial.replace('name=bulk_fuel', 'name=loop_fuel'),
                       self.serial.replace('round=1', 'round=0', 1)]:
            with self.assertRaises(ValueError):
                validate_m7(serial, self.m2, self.m6)

    def test_rejects_isolation_confusion_and_missing_native_tasks(self):
        for old, new in [('cpl=3', 'cpl=0'), ('cpl=0', 'cpl=3'),
                         ('root1=57344', 'root1=40960'), ('root0=4096 ', 'root0=8192 '),
                         ('M7TASK task=0', 'OMITTED task=0')]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                validate_m7(self.serial.replace(old, new, 1), self.m2, self.m6)

    def test_rejects_forged_savings_and_metadata(self):
        for old, new in [('pool_peak=112', 'pool_peak=108'), ('payload=4', 'payload=2'),
                         ('tables=14', 'tables=0'), ('after=89', 'after=90'),
                         ('maps=13', 'maps=8'), ('vm_context_bytes=272', 'vm_context_bytes=0'),
                         ('trace_bytes=520', 'trace_bytes=0')]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                validate_m7(self.serial.replace(old, new, 1), self.m2, self.m6)

    def test_rejects_corruption_fuel_and_bad_sampling(self):
        for old, new in [('sum1=40960', 'sum1=40961'), ('repeat1=40960', 'repeat1=0'),
                         ('fuel=65548', 'fuel=0'), ('read_ticks=10000', 'read_ticks=0'),
                         ('warmup=1', 'warmup=0'), ('shared=1', 'shared=0')]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                validate_m7(self.serial.replace(old, new, 1), self.m2, self.m6)

    def test_requires_matching_m6_control_and_image_scope(self):
        for field in ['pool', 'image']:
            m6 = copy.deepcopy(self.m6)
            if field == 'pool':
                m6['summary'][0]['pool_peak_frames'] += 1
            else:
                m6['config']['kernel_image_bytes'] += 4096
            with self.assertRaises(ValueError):
                validate_m7(self.serial, self.m2, m6)
