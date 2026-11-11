import unittest
from m14_validation import validate_m14


def fixture(bad=False):
    lines = []
    for case in range(8):
        if case < 4 and not bad:
            budget = [16,9,2,0][case]
            fuel = [4,4,4103] if budget >= 9 else ([4103,4103,8200] if budget == 2 else [8200]*3)
            lines.append(f'M14DONE case={case} dispatches={min(budget,9)} limited={int(budget<9)} ready=0 granted=24600 remaining={sum(fuel)} stops=3')
            for task in range(3):
                state = ('Halted(28672)' if task < 2 else 'Faulted(Authority)') if budget >= 9 else 'Cancelled'
                lines.append(f'M14STATE case={case} task={task} state={state} fuel={fuel[task]}')
            lines.append(f'M14STATE case={case} task=3 state=Empty fuel=0')
        else:
            error = {4:'Configuration',5:'Launch(Busy)',6:'Load(Version)',7:'Launch(Count)'}.get(case, 'Load(Code(Opcode))')
            if bad and case == 5: error = 'Load(Code(Opcode))'
            lines.append(f'M14START case={case} error={error}')
        live = int(case == 5)
        lines.append(f'M14ACCOUNT case={case} objects={live} payload={live} capabilities={live} private=0 mappings=0 preserved=1')
        reads = [4,4,2,0,0,0,0,0][case] if not bad else 0
        peak = 89 + int(case == 5 or (case < 4 and not bad))
        lines.append(f'M14CLEAN case={case} before=89 after=89 peak={peak} tables=73 mappings=30000 reads={reads} frame={16384 if reads else 0} kept={36864 if live else 0}')
    return '\n'.join(lines) + '\nM14CHECK cases=8 max_dispatches=1024 report_bytes=272 completion_bytes=176\nM14 PASS\n'


class M14ValidatorTests(unittest.TestCase):
    def validate(self, serial, bad=False):
        return validate_m14(serial, dict(allocated_frames=89,table_frames=73,virtual_mappings=30000),
                            'revocation-bad-opcode' if bad else 'normal')
    def test_accepts_normal_and_invalid_external_input(self):
        self.assertEqual(len(self.validate(fixture())['completed']), 4)
        self.assertEqual(self.validate(fixture(True), True)['completed'], [])
    def test_rejects_limit_state_resource_and_preexisting_data_errors(self):
        serial = fixture()
        for old,new in [('dispatches=9 limited=0','dispatches=10 limited=0'),
                        ('case=1 dispatches=9 limited=0','case=1 dispatches=9 limited=1'),
                        ('state=Cancelled','state=Ready'),('remaining=4111','remaining=24600'),
                        ('payload=0 capabilities=0','payload=1 capabilities=0'),
                        ('objects=1 payload=1 capabilities=1','objects=0 payload=0 capabilities=0'),
                        ('error=Launch(Busy)','error=Load(Version)'),('after=89','after=90'),
                        ('reads=4','reads=0'),('frame=16384','frame=0'),('kept=36864','kept=0'),
                        ('report_bytes=272','report_bytes=0')]:
            with self.subTest(old=old), self.assertRaises(ValueError): self.validate(serial.replace(old,new,1))
    def test_rejects_partial_or_duplicate_evidence_and_executed_bad_input(self):
        for serial in ['M14 PASS\n', fixture()+fixture(), fixture().replace('M14START case=6 error=Load(Version)\n','')]:
            with self.assertRaises(ValueError): self.validate(serial)
        for serial in [fixture(True)+'M14DONE case=0 dispatches=1 limited=0 ready=0 granted=24600 remaining=0 stops=3\n',
                       fixture(True).replace('peak=89','peak=90',1)]:
            with self.assertRaises(ValueError): self.validate(serial, True)
