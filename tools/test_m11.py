import json
from pathlib import Path
import unittest
from pack_bytecode import encode
from m9_validation import fingerprint
from m11_validation import validate_m11


def fixture(variant='normal'):
    spec = json.loads((Path(__file__).resolve().parent.parent / 'examples/bytecode/shared.json').read_text())
    length = 2048 if variant == 'shared-alt' else 4096
    for program in spec['programs']:
        program['instructions'][0][4] = length
    if variant == 'shared-bad-count':
        spec['programs'].pop()
    module = bytearray(encode(spec))
    if variant == 'shared-bad-opcode':
        module[-16] = 255
    serial = ('MEMMAP index=0 base=0x1000 length=0x1000 kind=6\n'
              f'M11MODULE bytes={len(module)} physical=0x1000 retained_pages=1 fingerprint={fingerprint(module)}\n')
    if variant in ('shared-bad-count', 'shared-bad-opcode'):
        reason = 'manifest=Count' if variant == 'shared-bad-count' else 'format=Code(Opcode)'
        return serial + f'M11REJECT {reason} admitted=0 grants=0\n', module
    serial += 'M11LAUNCH tasks=3 readers=2 payload=1 capabilities=3 code_bytes=144 fuel=12300 launch_bytes=80\n'
    value, fuel = length*7, 4100-length-2
    for task in range(3):
        serial += (f'M11READ task={task} allowed={int(task<2)} length={length} value={value if task<2 else 0} '
                   f'frame={"0x4000" if task<2 else "0x0"} pages={int(task<2)}\n')
        event = f'Ok(Event(Yield({value})))' if task<2 else 'Err(Authority)'
        serial += f'M11TASK task={task} result={event} before=4100 after={fuel if task<2 else fuel+1}\n'
    for task in range(2):
        serial += f'M11TASK task={task} result=Ok(Event(Halt({value}))) before={fuel} after={fuel-1}\n'
    serial += ('M11STOP stops=3 owner_caps=1 reads=3\n'
               'M11CLEAN turns=5 before=89 after=89 peak=90 tables=73 mappings=30000 capabilities=0 payload=0 code_bytes=0\nM11 PASS\n')
    return serial, module


class M11ValidatorTests(unittest.TestCase):
    def validate(self, serial, module, variant='normal'):
        return validate_m11(serial, dict(allocated_frames=89, table_frames=73, virtual_mappings=30000), module, variant)

    def test_all_variants(self):
        for variant in ('normal', 'shared-alt', 'shared-bad-count', 'shared-bad-opcode'):
            self.validate(*fixture(variant), variant)

    def test_rejects_copied_frame_ungranted_read_leak_and_wrong_fuel(self):
        serial, module = fixture()
        for old, new in [('frame=0x4000', 'frame=0x5000'), ('task=2 allowed=0', 'task=2 allowed=1'),
                         ('after=89', 'after=90'), ('owner_caps=1', 'owner_caps=0'), ('before=2 after=1', 'before=2 after=2'),
                         ('code_bytes=0', 'code_bytes=144'), ('kind=6', 'kind=0'), ('launch_bytes=80', 'launch_bytes=0')]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                self.validate(serial.replace(old,new,1), module)
        for changed in ['M11 PASS\n', serial+serial]:
            with self.assertRaises(ValueError): self.validate(changed,module)
        module[-1]=1
        with self.assertRaises(ValueError): self.validate(serial,module)

    def test_rejection_cannot_hide_partial_grants_or_execution(self):
        for variant in ('shared-bad-count','shared-bad-opcode'):
            serial,module=fixture(variant)
            for changed in [serial+'M11 PASS\n',serial+'M11READ task=0\n',serial.replace('grants=0','grants=1')]:
                with self.assertRaises(ValueError): self.validate(changed,module,variant)
