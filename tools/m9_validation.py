"""External module transport and execution evidence, with no ambient rights."""
import hashlib
import re
from m8_validation import single


def fingerprint(data):
    value = 0xcbf29ce484222325
    for byte in data:
        value = ((value ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
    return value


def validate_m9(serial, m2, module, variant):
    serial = serial.replace('\r', '')
    metadata = single(serial, 'M9MODULE', 'bytes physical retained_pages fingerprint'.split())
    address = metadata['physical']
    if (metadata['bytes'] != len(module) or len(module) != 224 or metadata['fingerprint'] != fingerprint(module)
            or address <= 0 or metadata['retained_pages'] != (address % 4096 + len(module) + 4095) // 4096):
        raise ValueError('M9 transport does not match the supplied package')
    regions = re.findall(r'^MEMMAP index=\d+ base=(0x[0-9a-f]+) length=(0x[0-9a-f]+) kind=6$', serial, re.M)
    if not any(int(base, 0) <= address and address + len(module) <= int(base, 0) + int(length, 0) for base, length in regions):
        raise ValueError('M9 module not inside retained kernel/module memory')
    if variant in ('module-bad-version', 'module-bad-opcode'):
        reason = 'Version' if variant == 'module-bad-version' else 'Code(Opcode)'
        expected = f'M9REJECT format={reason} admitted=0 cleanup=1'
        if (re.findall(r'^M9REJECT .+$', serial, re.M) != [expected]
                or re.search(r'^M9(?:LOAD|TASK|CLEAN| PASS)', serial, re.M)):
            raise ValueError('Invalid module was not rejected atomically')
        return dict(module=metadata, sha256=hashlib.sha256(module).hexdigest(), rejected=reason, admitted=0)
    if 'M9REJECT' in serial or serial.count('M9 PASS\n') != 1:
        raise ValueError('M9 module did not run to completion')
    load = single(serial, 'M9LOAD', 'tasks code_bytes fuel batch_bytes scheduler_bytes'.split())
    if load != dict(tasks=3, code_bytes=160, fuel=17, batch_bytes=1096, scheduler_bytes=608):
        raise ValueError('M9 code/fuel/metadata accounting mismatch')
    value = 99 if variant == 'module-alt' else 42
    expected = [f'M9TASK task=0 result=Ok(Event(Halt({value}))) before=4 after=2',
                'M9TASK task=1 result=Ok(Expired) before=5 after=3',
                'M9TASK task=2 result=Err(Authority) before=8 after=5',
                'M9TASK task=1 result=Ok(Event(Yield(3))) before=3 after=1',
                'M9TASK task=1 result=Ok(Event(Halt(3))) before=1 after=0']
    if re.findall(r'^M9TASK .+$', serial, re.M) != expected:
        raise ValueError('M9 task results, authority, scheduling or fuel mismatch')
    cleanup = single(serial, 'M9CLEAN', 'turns stops code_bytes tasks before after tables mappings capabilities payload'.split())
    if cleanup != dict(turns=5, stops=3, code_bytes=0, tasks=0, before=m2['allocated_frames'], after=m2['allocated_frames'],
                        tables=m2['table_frames'], mappings=m2['virtual_mappings'], capabilities=0, payload=0):
        raise ValueError('M9 code or runtime resources not reclaimed')
    return dict(module=metadata, sha256=hashlib.sha256(module).hexdigest(), load=load, task_results=expected, cleanup=cleanup)
