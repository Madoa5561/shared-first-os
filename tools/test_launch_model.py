import unittest
from launch_model_validation import expected_summary, validate_launch_model


class LaunchModelTests(unittest.TestCase):
    def fixture(self):
        digest, reads = expected_summary()
        return f'LAUNCHRESULT generated=240 rejected=264 digest={digest} reads={reads} before=89 after=89 peak=90 reject_peak=89 tables=73 mappings=30000 payload=0 capabilities=0\nLAUNCHMODEL PASS\n'
    def validate(self, serial):
        return validate_launch_model(serial, dict(allocated_frames=89,table_frames=73,virtual_mappings=30000))
    def test_accepts_independent_summary(self):
        self.validate(self.fixture())
    def test_rejects_missing_coverage_divergence_and_resource_leaks(self):
        serial = self.fixture()
        for old,new in [('generated=240','generated=239'),('rejected=264','rejected=263'),
                        ('after=89','after=90'),('reject_peak=89','reject_peak=90'),('payload=0','payload=1'),
                        (f'digest={expected_summary()[0]}','digest=0'),(f'reads={expected_summary()[1]}','reads=0')]:
            with self.subTest(old=old),self.assertRaises(ValueError): self.validate(serial.replace(old,new,1))
        for changed in ['LAUNCHMODEL PASS\n',serial+serial]:
            with self.assertRaises(ValueError): self.validate(changed)
