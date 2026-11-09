"""Independent external read-manifest, shared-frame and lifecycle checks."""
import hashlib
import re
from m6_validation import records
from m8_validation import single
from m9_validation import fingerprint


def validate_m11(serial, m2, module, variant):
    serial = serial.replace('\r', '')
    metadata = single(serial, 'M11MODULE', 'bytes physical retained_pages fingerprint'.split())
    address = metadata['physical']
    size = 144 if variant == 'shared-bad-count' else 208
    if (len(module) != size or metadata['bytes'] != size or metadata['fingerprint'] != fingerprint(module)
            or address <= 0 or metadata['retained_pages'] != (address % 4096 + size + 4095) // 4096):
        raise ValueError('M11 module transport mismatch')
    regions = re.findall(r'^MEMMAP index=\d+ base=(0x[0-9a-f]+) length=(0x[0-9a-f]+) kind=6$', serial, re.M)
    if not any(int(base, 0) <= address and address + size <= int(base, 0) + int(length, 0) for base, length in regions):
        raise ValueError('M11 module outside retained memory')
    result = dict(module=metadata, sha256=hashlib.sha256(module).hexdigest())
    if variant in ('shared-bad-count', 'shared-bad-opcode'):
        reason = 'manifest=Count' if variant == 'shared-bad-count' else 'format=Code(Opcode)'
        if (re.findall(r'^M11REJECT .+$', serial, re.M) != [f'M11REJECT {reason} admitted=0 grants=0']
                or re.search(r'^M11(?:LAUNCH|READ|TASK|STOP|CLEAN| PASS)', serial, re.M)):
            raise ValueError('M11 rejection was not atomic')
        return dict(result, rejected=reason)
    if 'M11REJECT' in serial or serial.count('M11 PASS\n') != 1:
        raise ValueError('M11 missing completion')
    launch = single(serial, 'M11LAUNCH', 'tasks readers payload capabilities code_bytes fuel launch_bytes'.split())
    if launch != dict(tasks=3, readers=2, payload=1, capabilities=3, code_bytes=144, fuel=12300, launch_bytes=80):
        raise ValueError('M11 grant/code accounting mismatch')
    length = 2048 if variant == 'shared-alt' else 4096
    value = length * 7
    reads = records(serial, 'M11READ', set('task allowed length value frame pages'.split()))
    if len(reads) != 3:
        raise ValueError('M11 read count mismatch')
    frame = reads[0]['frame']
    expected = [dict(task=task, allowed=int(task < 2), length=length, value=value if task < 2 else 0,
                     frame=frame if task < 2 else 0, pages=int(task < 2)) for task in range(3)]
    if frame <= 0 or frame % 4096 or reads != expected:
        raise ValueError('M11 physical sharing or authority mismatch')
    fuel = 4100-length-2
    events = [f'M11TASK task={task} result=Ok(Event(Yield({value}))) before=4100 after={fuel}' for task in range(2)]
    events += [f'M11TASK task=2 result=Err(Authority) before=4100 after={fuel+1}']
    events += [f'M11TASK task={task} result=Ok(Event(Halt({value}))) before={fuel} after={fuel-1}' for task in range(2)]
    if re.findall(r'^M11TASK .+$', serial, re.M) != events:
        raise ValueError('M11 external execution or fuel mismatch')
    if single(serial, 'M11STOP', 'stops owner_caps reads'.split()) != dict(stops=3, owner_caps=1, reads=3):
        raise ValueError('M11 task/owner cleanup mismatch')
    cleanup = single(serial, 'M11CLEAN', 'turns before after peak tables mappings capabilities payload code_bytes'.split())
    if cleanup != dict(turns=5, before=m2['allocated_frames'], after=m2['allocated_frames'], peak=m2['allocated_frames']+1,
                       tables=m2['table_frames'], mappings=m2['virtual_mappings'], capabilities=0, payload=0, code_bytes=0):
        raise ValueError('M11 resources not reclaimed')
    return dict(result, launch=launch, reads=reads, task_results=events, cleanup=cleanup)
