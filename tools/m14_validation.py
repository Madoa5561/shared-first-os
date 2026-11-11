"""Validate bounded launch results, atomic errors and physical cleanup."""
import re
from m6_validation import records
from m8_validation import single


def validate_m14(serial, m2, variant):
    serial = serial.replace('\r', '')
    if serial.count('M14 PASS\n') != 1:
        raise ValueError('Missing M14 completion')
    bad = variant == 'revocation-bad-opcode'
    expected_errors = ([f'M14START case={case} error=Load(Code(Opcode))' for case in range(4)] if bad else [])
    expected_errors += ['M14START case=4 error=Configuration',
                        f'M14START case=5 error={"Load(Code(Opcode))" if bad else "Launch(Busy)"}',
                        'M14START case=6 error=Load(Version)', 'M14START case=7 error=Launch(Count)']
    if re.findall(r'^M14START .+$', serial, re.M) != expected_errors:
        raise ValueError('M14 wrong rejection stage or executed invalid code')
    done = records(serial, 'M14DONE', set('case dispatches limited ready granted remaining stops'.split()))
    if done != ([] if bad else [dict(case=case,dispatches=[9,9,2,0][case],limited=int(case>=2),ready=0,
                                    granted=24600,remaining=[4111,4111,16406,24600][case],stops=3) for case in range(4)]):
        raise ValueError('M14 execution bound, fuel or cleanup mismatch')
    states = []
    if not bad:
        for case in range(4):
            for task in range(3):
                state = ('Faulted(Authority)' if task == 2 else 'Halted(28672)') if case < 2 else 'Cancelled'
                fuel = (4103 if task == 2 else 4) if case < 2 else (4103 if case == 2 and task < 2 else 8200)
                states.append(f'M14STATE case={case} task={task} state={state} fuel={fuel}')
            states.append(f'M14STATE case={case} task=3 state=Empty fuel=0')
    if re.findall(r'^M14STATE .+$', serial, re.M) != states:
        raise ValueError('M14 terminal snapshot mismatch')
    accounts = records(serial, 'M14ACCOUNT', set('case objects payload capabilities private mappings preserved'.split()))
    if accounts != [dict(case=case,objects=int(case==5),payload=int(case==5),capabilities=int(case==5),
                         private=0,mappings=0,preserved=1) for case in range(8)]:
        raise ValueError('M14 leaked acquired resources or consumed busy store')
    cleanup = records(serial, 'M14CLEAN', set('case before after peak tables mappings reads frame kept'.split()))
    if len(cleanup) != 8:
        raise ValueError('M14 incomplete physical evidence')
    for case, entry in enumerate(cleanup):
        reads = 0 if bad else [4,4,2,0,0,0,0,0][case]
        frame = entry['frame']
        if (reads and (frame <= 0 or frame % 4096)) or (not reads and frame != 0):
            raise ValueError('M14 missing backing or unexpected read')
        extra = int(case==5 or (not bad and case<4))
        if entry != dict(case=case,before=m2['allocated_frames'],after=m2['allocated_frames'],peak=m2['allocated_frames']+extra,
                         tables=m2['table_frames'],mappings=m2['virtual_mappings'],reads=reads,frame=frame,kept=36864 if case==5 else 0):
            raise ValueError('M14 physical balance or busy data changed')
    check = single(serial, 'M14CHECK', 'cases max_dispatches report_bytes completion_bytes'.split())
    if (check['cases'] != 8 or check['max_dispatches'] != 1024
            or not 128 <= check['completion_bytes'] <= 256
            or not check['completion_bytes'] + 96 <= check['report_bytes'] <= 512):
        raise ValueError('M14 metadata or execution limit mismatch')
    return dict(errors=expected_errors,completed=done,accounts=accounts,cleanup=cleanup,check=check)
