import unittest
from m10_validation import validate_m10


def fixture():
    return '\n'.join(f'M10CASE case={case} turns={int(case!=0)} stops=3 calls={int(case==3)} before=89 after=89 peak=90 tables=73 mappings=30000 payload=0 capabilities=0 code_bytes=0' for case in range(4)) + '\nM10CHECK cases=4 session_bytes=616 host_bound=1 drop_cleanup=1 once=1 code_lifetime=1\nM10 PASS\n'


class M10ValidatorTests(unittest.TestCase):
    def validate(self, serial):
        return validate_m10(serial, dict(allocated_frames=89, table_frames=73, virtual_mappings=30000))

    def test_accepts_all_four_lifecycle_cases(self):
        self.assertEqual(len(self.validate(fixture())['cases']), 4)

    def test_rejects_missing_case_double_stop_leak_or_wrong_peak(self):
        for serial in ['M10 PASS\n', fixture()+fixture(), fixture().replace('case=2', 'case=1'),
                       fixture().replace('stops=3', 'stops=4', 1), fixture().replace('after=89', 'after=90', 1),
                       fixture().replace('peak=90', 'peak=89', 1), fixture().replace('code_bytes=0', 'code_bytes=48', 1),
                       fixture().replace('capabilities=0', 'capabilities=1', 1), fixture().replace('session_bytes=616', 'session_bytes=0')]:
            with self.assertRaises(ValueError):
                self.validate(serial)
