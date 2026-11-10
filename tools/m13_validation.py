"""Independent expected observations for two bounded external execution cases."""
import re
from m6_validation import records
from m8_validation import single


def validate_m13(serial, m2, variant):
    serial = serial.replace('\r', '')
    if variant == 'revocation-bad-opcode':
        if re.findall(r'^M13.*$', serial, re.M) != ['M13REJECT format=Code(Opcode) admitted=0 grants=0']:
            raise ValueError('M13 rejection executed or acquired resources')
        return dict(rejected='Code(Opcode)')
    if 'M13REJECT' in serial or serial.count('M13 PASS\n') != 1:
        raise ValueError('Missing M13 completion')
    # Each row is (action, task states, remaining fuel, capability count, revoked).
    timelines = [
        [('initial', 'RRR', (8200,8200,8200), 3,0),
         ('dispatch','RRR',(4103,8200,8200),3,0),
         ('dispatch','RRR',(4103,4103,8200),3,0),
         ('dispatch','RRF',(4103,4103,4103),3,0),
         ('dispatch','RRF',(4102,4103,4103),3,0),
         ('dispatch','RRF',(4102,4102,4103),3,0),
         ('dispatch','RRF',(5,4102,4103),3,0),
         ('dispatch','RRF',(5,5,4103),3,0),
         ('dispatch','HRF',(4,5,4103),2,0),
         ('dispatch','HHF',(4,4,4103),1,0),
         ('stable','HHF',(4,4,4103),1,0)],
        [('initial','RRR',(8200,8200,8200),3,0),
         ('dispatch','RRR',(4103,8200,8200),3,0),
         ('dispatch','RRR',(4103,4103,8200),3,0),
         ('cancel','CRR',(4103,4103,8200),2,0),
         ('revoke','CRR',(4103,4103,8200),0,1),
         ('dispatch','CRF',(4103,4103,4103),0,1),
         ('dispatch','CRF',(4103,4102,4103),0,1),
         ('dispatch','CFF',(4103,5,4103),0,1),
         ('stable','CFF',(4103,5,4103),0,1)]
    ]
    names = dict(R='Ready', H='Halted(28672)', F='Faulted(Authority)', C='Cancelled')
    expected = []
    for case, timeline in enumerate(timelines):
        for step, (action, states, fuel, caps, revoked) in enumerate(timeline):
            expected.append(f'M13SNAP case={case} step={step} action={action} admitted=3 ready={states.count("R")} granted=24600 remaining={sum(fuel)} objects={1-revoked} payload={1-revoked} capabilities={caps} mappings=0 revoked={revoked}')
            for task, state in enumerate(states):
                expected.append(f'M13STATE case={case} step={step} task={task} state={names[state]} fuel={fuel[task]}')
            expected.append(f'M13STATE case={case} step={step} task=3 state=Empty fuel=0')
    if re.findall(r'^M13(?:SNAP|STATE) .+$', serial, re.M) != expected:
        raise ValueError('M13 inconsistent, mutated or missing observation')
    events = [
        [(1,0,'Ok(Expired)',8200,4103),(2,1,'Ok(Expired)',8200,4103),(3,2,'Err(Authority)',8200,4103),
         (4,0,'Ok(Event(Yield(28672)))',4103,4102),(5,1,'Ok(Event(Yield(28672)))',4103,4102),
         (6,0,'Ok(Expired)',4102,5),(7,1,'Ok(Expired)',4102,5),
         (8,0,'Ok(Event(Halt(28672)))',5,4),(9,1,'Ok(Event(Halt(28672)))',5,4)],
        [(1,0,'Ok(Expired)',8200,4103),(2,1,'Ok(Expired)',8200,4103),(5,2,'Err(Authority)',8200,4103),
         (6,1,'Ok(Event(Yield(28672)))',4103,4102),(7,1,'Err(Authority)',4102,5)]
    ]
    expected_events = [f'M13EVENT case={case} step={step} task={task} result={result} before={before} after={after}'
                       for case, sequence in enumerate(events) for step, task, result, before, after in sequence]
    if re.findall(r'^M13EVENT .+$', serial, re.M) != expected_events:
        raise ValueError('M13 observation changed scheduling or results')
    reads = records(serial, 'M13READ', set('case task allowed length value frame pages'.split()))
    expected_reads = []
    for case, pattern in enumerate([[(0,1),(1,1),(2,0),(0,1),(1,1)], [(0,1),(1,1),(2,0),(1,0)]]):
        allowed = [entry for entry in reads if entry['case'] == case and entry['allowed']]
        if not allowed or allowed[0]['frame'] <= 0 or allowed[0]['frame'] % 4096:
            raise ValueError('M13 missing shared backing')
        frame = allowed[0]['frame']
        expected_reads += [dict(case=case,task=task,allowed=permit,length=4096,value=28672 if permit else 0,
                                frame=frame if permit else 0,pages=permit) for task,permit in pattern]
    if reads != expected_reads:
        raise ValueError('M13 read authority or backing mismatch')
    cleanup = records(serial, 'M13CLEAN', set('case stops before after peak tables mappings payload capabilities'.split()))
    if cleanup != [dict(case=case,stops=3,before=m2['allocated_frames'],after=m2['allocated_frames'],
                        peak=m2['allocated_frames']+1,tables=m2['table_frames'],mappings=m2['virtual_mappings'],
                        payload=0,capabilities=0) for case in range(2)]:
        raise ValueError('M13 cleanup imbalance')
    check = single(serial, 'M13CHECK', 'cases scheduler_bytes session_bytes snapshot_bytes inspection_bytes code_bytes'.split())
    if check != dict(cases=2,scheduler_bytes=640,session_bytes=648,snapshot_bytes=128,inspection_bytes=168,code_bytes=0):
        raise ValueError('M13 metadata accounting mismatch')
    return dict(snapshots=20,task_observations=80,events=expected_events,reads=reads,cleanup=cleanup,check=check)
