import json
import unittest
from pathlib import Path
from pack_bytecode import encode
from m9_validation import fingerprint, validate_m9


def fixture(variant='normal'):
    spec = json.loads((Path(__file__).resolve().parent.parent / 'examples/bytecode/demo.json').read_text())
    if variant == 'module-alt':
        spec['programs'][0]['instructions'][0][4] = 99
    module = bytearray(encode(spec))
    if variant == 'module-bad-version':
        module[8] = 2
    if variant == 'module-bad-opcode':
        module[-16] = 255
    serial = ('MEMMAP index=0 base=0x1000 length=0x1000 kind=6\n'
              f'M9MODULE bytes=224 physical=0x1000 retained_pages=1 fingerprint={fingerprint(module)}\n')
    if variant in ('module-bad-version', 'module-bad-opcode'):
        reason = 'Version' if variant == 'module-bad-version' else 'Code(Opcode)'
        return serial + f'M9REJECT format={reason} admitted=0 cleanup=1\n', module
    value = 99 if variant == 'module-alt' else 42
    serial += f'''M9LOAD tasks=3 code_bytes=160 fuel=17 batch_bytes=1096 scheduler_bytes=608
M9TASK task=0 result=Ok(Event(Halt({value}))) before=4 after=2
M9TASK task=1 result=Ok(Expired) before=5 after=3
M9TASK task=2 result=Err(Authority) before=8 after=5
M9TASK task=1 result=Ok(Event(Yield(3))) before=3 after=1
M9TASK task=1 result=Ok(Event(Halt(3))) before=1 after=0
M9CLEAN turns=5 stops=3 code_bytes=0 tasks=0 before=89 after=89 tables=73 mappings=30000 capabilities=0 payload=0
M9 PASS
'''
    return serial, module


class M9ValidatorTests(unittest.TestCase):
    def validate(self, serial, module, variant='normal'):
        return validate_m9(serial, dict(allocated_frames=89, table_frames=73, virtual_mappings=30000), module, variant)

    def test_all_variants(self):
        for variant in ['normal', 'module-alt', 'module-bad-version', 'module-bad-opcode']:
            self.validate(*fixture(variant), variant)

    def test_rejects_missing_duplicate_and_tampered_transport(self):
        serial, module = fixture()
        for changed in ['M9 PASS\n', serial + serial, serial.replace('fingerprint=', 'missing='),
                        serial.replace('kind=6', 'kind=0'), serial.replace('physical=0x1000', 'physical=0x3000')]:
            with self.assertRaises(ValueError):
                self.validate(changed, module)
        module[-1] = 1
        with self.assertRaises(ValueError):
            self.validate(serial, module)

    def test_rejects_fake_execution_authority_or_cleanup(self):
        serial, module = fixture()
        for old, new in [('Halt(42)', 'Halt(99)'), ('Err(Authority)', 'Ok(Expired)'),
                         ('after=89', 'after=90'), ('code_bytes=0', 'code_bytes=160'),
                         ('batch_bytes=1096', 'batch_bytes=0'), ('before=3 after=1', 'before=5 after=3'),
                         ('M9TASK task=1 result=Ok(Expired)', 'OMITTED task=1 result=Ok(Expired)')]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                self.validate(serial.replace(old, new, 1), module)

    def test_rejection_cannot_hide_partial_admission(self):
        for variant in ['module-bad-version', 'module-bad-opcode']:
            serial, module = fixture(variant)
            for changed in [serial + 'M9 PASS\n', serial + 'M9TASK task=0\n',
                            serial.replace('admitted=0', 'admitted=1'), serial.replace('cleanup=1', 'cleanup=0')]:
                with self.assertRaises(ValueError):
                    self.validate(changed, module, variant)
