import json
from pathlib import Path
import unittest
from pack_bytecode import encode
from m9_validation import fingerprint
from m12_validation import validate_m12


def fixture(bad=False):
    module=bytearray(encode(json.loads((Path(__file__).resolve().parent.parent/'examples/bytecode/revocation.json').read_text())))
    if bad: module[-16]=255
    serial=f'MEMMAP index=0 base=0x1000 length=0x1000 kind=6\nM12MODULE bytes=256 physical=0x1000 retained_pages=1 fingerprint={fingerprint(module)}\n'
    if bad: return serial+'M12REJECT format=Code(Opcode) admitted=0 grants=0\n',module
    sequences=[[(0,1,'Yield',8200,4102),(1,1,'Yield',8200,4102),(2,0,'Fault',8200,4103),(0,0,'Fault',4102,5),(1,0,'Fault',4102,5)],
               [(0,1,'Yield',8200,4102),(1,1,'Yield',8200,4102),(2,0,'Fault',8200,4103),(1,0,'Fault',4102,5)],
               [(0,0,'Fault',8200,4103),(1,1,'Yield',8200,4102),(2,0,'Fault',8200,4103),(1,1,'Halt',4102,4)]]
    for case, sequence in enumerate(sequences):
        if case==2: serial+='M12RESTART old=65536 new=196608 foreign=196608\n'
        for index,(task,permit,outcome,before,after) in enumerate(sequence):
            if case<2 and index==2:
                serial+=f'M12REVOKE case={case} changed=1 before=90 after=89 payload=0 capabilities=0\nM12REVOKE case={case} changed=0 before=89 after=89 payload=0 capabilities=0\nM12CONTROL case={case} ready={3-case} cancelled={case} repeat_cancel=0\n'
            serial+=f'M12READ case={case} task={task} allowed={permit} length=4096 value={28672 if permit else 0} frame={"0x4000" if permit else "0x0"} pages={permit}\n'
            result='Err(Authority)' if outcome=='Fault' else f'Ok(Event({outcome}(28672)))'
            serial+=f'M12TASK case={case} task={task} result={result} before={before} after={after}\n'
        serial+=f'M12STOP case={case} stops=3 owner_caps={int(case==2)} reads={len(sequence)}\nM12FRAMES case={case} turns={len(sequence)} before=89 after=89 peak=90 tables=73 mappings=30000 payload=0 capabilities=0\n'
    return serial+'M12CLEAN cases=3 code_bytes=0 tasks=0\nM12 PASS\n',module


class M12ValidatorTests(unittest.TestCase):
    def validate(self,serial,module,variant='normal'):
        return validate_m12(serial,dict(allocated_frames=89,table_frames=73,virtual_mappings=30000),module,variant)
    def test_accepts_three_cases_and_atomic_rejection(self):
        self.validate(*fixture());self.validate(*fixture(True),'revocation-bad-opcode')
    def test_rejects_stale_success_mismatched_frames_refill_or_leak(self):
        serial,module=fixture()
        for old,new in [('case=2 task=0 allowed=0','case=2 task=0 allowed=1'),('frame=0x4000','frame=0x5000'),
                        ('before=4102 after=5','before=4102 after=6'),('changed=1 before=90 after=89','changed=1 before=90 after=90'),
                        ('old=65536 new=196608','old=65536 new=65536'),('stops=3','stops=4'),('code_bytes=0','code_bytes=192'),
                        ('kind=6','kind=0')]:
            with self.subTest(old=old),self.assertRaises(ValueError):self.validate(serial.replace(old,new,1),module)
        for changed in [serial+serial,'M12 PASS\n']:
            with self.assertRaises(ValueError):self.validate(changed,module)
    def test_rejection_cannot_hide_any_execution_or_grants(self):
        serial,module=fixture(True)
        for changed in [serial+'M12 PASS\n',serial+'M12READ case=0\n',serial.replace('grants=0','grants=1')]:
            with self.assertRaises(ValueError):self.validate(changed,module,'revocation-bad-opcode')
