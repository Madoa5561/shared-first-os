"""Synthetic negative tests for the validator; not QEMU boot evidence."""
import unittest
from m8_validation import validate_m8


def fixture():
    return '''M8CONFIG capacity=4 quantum=2 max_quantum=64 max_batch_fuel=2097152 scheduler_bytes=640 host_bytes=72
M8DISPATCH scenario=0 task=0 result=0 value=0 before=5 after=3
M8DISPATCH scenario=0 task=1 result=2 value=42 before=2 after=0
M8DISPATCH scenario=0 task=2 result=1 value=0 before=3 after=2
M8DISPATCH scenario=0 task=3 result=5 value=0 before=2 after=0
M8DISPATCH scenario=0 task=0 result=0 value=0 before=3 after=1
M8DISPATCH scenario=0 task=2 result=1 value=0 before=2 after=0
M8DISPATCH scenario=0 task=0 result=3 value=0 before=1 after=0
M8DISPATCH scenario=0 task=2 result=3 value=0 before=0 after=0
M8DISPATCH scenario=1 task=0 result=0 value=0 before=5 after=3
M8DISPATCH scenario=1 task=1 result=0 value=0 before=10000 after=5902
M8DISPATCH scenario=1 task=2 result=0 value=0 before=10000 after=5902
M8DISPATCH scenario=1 task=3 result=4 value=0 before=10000 after=5902
M8SHARED frame1=0x1000 frame2=0x1000 payload=1 capabilities=3 mappings=0 sum1=28672 sum2=28672
M8DISPATCH scenario=1 task=0 result=0 value=0 before=3 after=1
M8DISPATCH scenario=1 task=1 result=4 value=0 before=5902 after=1805
M8DISPATCH scenario=1 task=2 result=4 value=0 before=5902 after=1805
M8DISPATCH scenario=1 task=0 result=3 value=0 before=1 after=0
M8DISPATCH scenario=2 task=1 result=2 value=0 before=1 after=0
M8QUOTAS zero=1 overflow=1 aggregate=1 capacity=1 quantum=1 no_refund=1 no_reuse=1 cancel=1 shutdown=1
M8FRAMES before=89 after=89 tables_before=73 tables_after=73 mappings_before=30000 mappings_after=30000 payload=0 capabilities=0
M8CHECK dispatches=17 fairness=1 fault_isolation=1 shared=1 revocation=1 cleanup=1
M8 PASS
'''


class M8ValidatorTests(unittest.TestCase):
    def validate(self, serial):
        return validate_m8(serial, dict(allocated_frames=89, table_frames=73, virtual_mappings=30000))

    def test_accepts_full_evidence(self):
        self.assertEqual(len(self.validate(fixture())['dispatches']), 17)

    def test_marker_missing_duplicate_or_truncated_records_fail(self):
        for serial in ['M8 PASS\n', fixture() + fixture(), fixture().replace('M8 PASS\n', ''),
                       fixture().replace('M8DISPATCH ', 'OMITTED ', 1),
                       fixture().replace('M8QUOTAS ', 'OMITTED ', 1),
                       fixture().replace('M8FRAMES ', 'OMITTED ', 1)]:
            with self.assertRaises(ValueError):
                self.validate(serial)

    def test_rejects_starvation_refill_wrong_identity_and_missing_revoke(self):
        for old, new in [('task=1 result=2', 'task=0 result=2'), ('after=3', 'after=5'),
                         ('result=4', 'result=0'), ('before=5902', 'before=10000'),
                         ('value=42', 'value=0')]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                self.validate(fixture().replace(old, new, 1))

    def test_rejects_fake_sharing_leaks_and_bad_quotas(self):
        for old, new in [('frame2=0x1000', 'frame2=0x2000'), ('sum1=28672', 'sum1=0'),
                         ('after=89', 'after=90'), ('capabilities=0', 'capabilities=1'),
                         ('no_reuse=1', 'no_reuse=0'), ('scheduler_bytes=640', 'scheduler_bytes=0'),
                         ('quantum=2 ', 'quantum=0 '), ('host_bytes=72', 'host_bytes=0')]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                self.validate(fixture().replace(old, new, 1))
