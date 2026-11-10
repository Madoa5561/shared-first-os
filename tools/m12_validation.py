"""Scoped revocation and stale-generation restart evidence."""
import hashlib
import re
from m6_validation import records
from m8_validation import single
from m9_validation import fingerprint


def validate_m12(serial, m2, module, variant):
    serial = serial.replace('\r', '')
    metadata = single(serial, 'M12MODULE', 'bytes physical retained_pages fingerprint'.split())
    address = metadata['physical']
    if (len(module) != 256 or metadata['bytes'] != 256 or metadata['fingerprint'] != fingerprint(module)
            or address <= 0 or metadata['retained_pages'] != (address % 4096 + 256 + 4095) // 4096):
        raise ValueError('M12 module mismatch')
    regions = re.findall(r'^MEMMAP index=\d+ base=(0x[0-9a-f]+) length=(0x[0-9a-f]+) kind=6$', serial, re.M)
    if not any(int(base, 0) <= address and address + 256 <= int(base, 0) + int(length, 0) for base, length in regions):
        raise ValueError('M12 module outside retained memory')
    result = dict(module=metadata, sha256=hashlib.sha256(module).hexdigest())
    if variant == 'revocation-bad-opcode':
        if (re.findall(r'^M12REJECT .+$', serial, re.M) != ['M12REJECT format=Code(Opcode) admitted=0 grants=0']
                or re.search(r'^M12(?:READ|TASK|REVOKE|RESTART|CONTROL|STOP|FRAMES|CLEAN| PASS)', serial, re.M)):
            raise ValueError('M12 invalid code acquired authority or executed')
        return dict(result, rejected='Code(Opcode)')
    if 'M12REJECT' in serial or serial.count('M12 PASS\n') != 1:
        raise ValueError('M12 missing completion')
    reads = records(serial, 'M12READ', set('case task allowed length value frame pages'.split()))
    patterns = [[(0,1),(1,1),(2,0),(0,0),(1,0)],[(0,1),(1,1),(2,0),(1,0)],[(0,0),(1,1),(2,0),(1,1)]]
    expected_reads = []
    for case, pattern in enumerate(patterns):
        allowed = [entry for entry in reads if entry['case'] == case and entry['allowed']]
        if not allowed or allowed[0]['frame'] <= 0 or allowed[0]['frame'] % 4096:
            raise ValueError('M12 missing actual shared backing')
        frame = allowed[0]['frame']
        expected_reads += [dict(case=case,task=task,allowed=permit,length=4096,value=28672 if permit else 0,
                                frame=frame if permit else 0,pages=permit) for task,permit in pattern]
    if reads != expected_reads:
        raise ValueError('M12 revoked/stale/foreign read or backing mismatch')
    events = []
    for case in range(3):
        sequence = ([(0,'Yield',8200,4102),(1,'Yield',8200,4102),(2,'Fault',8200,4103)]
                    + ([(0,'Fault',4102,5)] if case == 0 else []) + [(1,'Fault',4102,5)]) if case < 2 else [
                        (0,'Fault',8200,4103),(1,'Yield',8200,4102),(2,'Fault',8200,4103),(1,'Halt',4102,4)]
        for task,outcome,before,after in sequence:
            outcome = 'Err(Authority)' if outcome == 'Fault' else f'Ok(Event({outcome}(28672)))'
            events.append(f'M12TASK case={case} task={task} result={outcome} before={before} after={after}')
    if re.findall(r'^M12TASK .+$',serial,re.M) != events:
        raise ValueError('M12 resumption/fuel mismatch')
    baseline = m2['allocated_frames']
    revocations = records(serial,'M12REVOKE',set('case changed before after payload capabilities'.split()))
    if revocations != [dict(case=case,changed=changed,before=baseline+changed,after=baseline,payload=0,capabilities=0)
                       for case in range(2) for changed in (1,0)]:
        raise ValueError('M12 revocation did not synchronously reclaim backing')
    controls = records(serial,'M12CONTROL',set('case ready cancelled repeat_cancel'.split()))
    if controls != [dict(case=case,ready=3-case,cancelled=case,repeat_cancel=0) for case in range(2)]:
        raise ValueError('M12 cancellation/readiness mismatch')
    restart = single(serial,'M12RESTART','old new foreign'.split())
    if (any(value >> 48 or value >> 16 == 0 for value in restart.values())
            or restart['old'] & 65535 != restart['new'] & 65535 or restart['new'] >> 16 <= restart['old'] >> 16):
        raise ValueError('M12 stale handle did not advance generation')
    stops = records(serial,'M12STOP',set('case stops owner_caps reads'.split()))
    if stops != [dict(case=case,stops=3,owner_caps=int(case==2),reads=5 if case==0 else 4) for case in range(3)]:
        raise ValueError('M12 stop count or owner lifetime mismatch')
    frames = records(serial,'M12FRAMES',set('case turns before after peak tables mappings payload capabilities'.split()))
    if frames != [dict(case=case,turns=5 if case==0 else 4,before=baseline,after=baseline,peak=baseline+1,
                       tables=m2['table_frames'],mappings=m2['virtual_mappings'],payload=0,capabilities=0) for case in range(3)]:
        raise ValueError('M12 leaked physical or mapping resources')
    if single(serial,'M12CLEAN','cases code_bytes tasks'.split()) != dict(cases=3,code_bytes=0,tasks=0):
        raise ValueError('M12 code not released')
    return dict(result,reads=reads,task_results=events,revocations=revocations,restart=restart,frames=frames)
