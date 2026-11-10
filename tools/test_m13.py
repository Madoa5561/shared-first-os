import unittest
from m13_validation import validate_m13


def fixture():
    lines = []
    for case in range(2):
        states = ['Ready'] * 3
        fuel = [8200] * 3
        caps, revoked, step = 3, 0, 0
        def observe(action):
            lines.append(f'M13SNAP case={case} step={step} action={action} admitted=3 ready={states.count("Ready")} granted=24600 remaining={sum(fuel)} objects={1-revoked} payload={1-revoked} capabilities={caps} mappings=0 revoked={revoked}')
            for task in range(3):
                lines.append(f'M13STATE case={case} step={step} task={task} state={states[task]} fuel={fuel[task]}')
            lines.append(f'M13STATE case={case} step={step} task=3 state=Empty fuel=0')
        observe('initial')
        schedule = [(0,'read'),(1,'read'),(2,'deny'),(0,'yield'),(1,'yield'),(0,'read'),(1,'read'),(0,'halt'),(1,'halt')] if case == 0 else [(0,'read'),(1,'read'),(0,'cancel'),(0,'revoke'),(2,'deny'),(1,'yield'),(1,'deny')]
        for task, action in schedule:
            step += 1
            before = fuel[task]
            if action == 'cancel':
                states[task] = 'Cancelled'
                caps -= 1
            elif action == 'revoke':
                caps, revoked = 0, 1
            else:
                fuel[task] -= 4097 if action in ('read', 'deny') else 1
                if action == 'deny': states[task] = 'Faulted(Authority)'
                if action == 'halt':
                    states[task] = 'Halted(28672)'
                    caps -= 1
                result = dict(read='Ok(Expired)',deny='Err(Authority)',yield_='Ok(Event(Yield(28672)))',halt='Ok(Event(Halt(28672)))')[action if action != 'yield' else 'yield_']
                if action in ('read', 'deny'):
                    permit = int(action == 'read')
                    lines.append(f'M13READ case={case} task={task} allowed={permit} length=4096 value={28672*permit} frame={16384*permit} pages={permit}')
                lines.append(f'M13EVENT case={case} step={step} task={task} result={result} before={before} after={fuel[task]}')
            observe(action if action in ('cancel', 'revoke') else 'dispatch')
        step += 1
        observe('stable')
        lines.append(f'M13CLEAN case={case} stops=3 before=89 after=89 peak=90 tables=73 mappings=30000 payload=0 capabilities=0')
    return '\n'.join(lines) + '\nM13CHECK cases=2 scheduler_bytes=640 session_bytes=648 snapshot_bytes=128 inspection_bytes=168 code_bytes=0\nM13 PASS\n'


class M13ValidatorTests(unittest.TestCase):
    def validate(self, serial, variant='normal'):
        return validate_m13(serial, dict(allocated_frames=89,table_frames=73,virtual_mappings=30000), variant)
    def test_accepts_complete_observations_and_rejection(self):
        self.assertEqual(self.validate(fixture())['snapshots'], 20)
        self.validate('M13REJECT format=Code(Opcode) admitted=0 grants=0\n', 'revocation-bad-opcode')
    def test_rejects_wrong_state_fuel_resources_or_backing(self):
        serial = fixture()
        for old, new in [('state=Halted(28672)', 'state=Cancelled'), ('state=Cancelled', 'state=Faulted(Authority)'),
                         ('remaining=24600','remaining=24601'), ('state=Ready fuel=4103','state=Ready fuel=8200'),
                         ('state=Empty fuel=0','state=Ready fuel=0'), ('payload=0 capabilities=0','payload=1 capabilities=0'),
                         ('revoked=1','revoked=0'), ('frame=16384','frame=20480'), ('stops=3','stops=4'),
                         ('after=89','after=90'), ('snapshot_bytes=128','snapshot_bytes=0'), ('result=Ok(Expired)','result=Err(Fuel)')]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                self.validate(serial.replace(old, new, 1))
    def test_rejects_missing_duplicate_or_executed_rejection(self):
        serial = fixture()
        for changed in [serial+serial, 'M13 PASS\n', serial.replace('M13STATE case=0 step=0 task=3 state=Empty fuel=0\n','')]:
            with self.assertRaises(ValueError): self.validate(changed)
        rejected = 'M13REJECT format=Code(Opcode) admitted=0 grants=0\n'
        for changed in [rejected+'M13 PASS\n', rejected+'M13SNAP case=0\n', rejected.replace('grants=0','grants=1')]:
            with self.assertRaises(ValueError): self.validate(changed, 'revocation-bad-opcode')
