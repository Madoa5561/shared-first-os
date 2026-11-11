"""Independent summary oracle for the generated launch corpus."""
from m8_validation import single


def expected_summary():
    digest = reads = 0
    for seed in range(16):
        for quantum in (1, 2, 64):
            for limit in (0, 1, 2, 7, 32):
                fuel = [2 + (seed * 3 + task) % 11 for task in range(3)]
                state = [None] * 3
                pc, value = [0]*3, [0]*3
                queue = [0, 1, 2]
                turns = 0
                while queue and turns < limit:
                    task = queue.pop(0)
                    turns += 1
                    kind = (seed + task) % 4
                    for _ in range(quantum):
                        position = pc[task]
                        reading = kind == 1 and position in (0, 2)
                        cost = seed % 4 + 2 if reading else 1
                        if cost > fuel[task]:
                            state[task] = 4000
                            break
                        fuel[task] -= cost
                        pc[task] += 1
                        if kind == 2:
                            pc[task] = 0
                            continue
                        if kind == 3 and position == 1:
                            state[task] = 3000
                            break
                        if reading:
                            if not seed & (1 << task):
                                state[task] = 2000
                                break
                            reads += 1
                            value[task] = (seed + 1) * (seed % 4 + 1)
                        elif kind == 0 and position == 0: value[task] = seed + task
                        elif kind == 0 and position == 2: value[task] *= 2
                        if position == 3:
                            state[task] = 1000 + value[task]
                            break
                        if position == 1 and kind < 2: break
                    if state[task] is None: queue.append(task)
                contribution = turns + bool(queue) * 97
                for task in range(3):
                    contribution = contribution * 31 + (state[task] or 5000) + fuel[task]
                digest += contribution
    return digest, reads


def validate_launch_model(serial, m2):
    serial = serial.replace('\r', '')
    if serial.count('LAUNCHMODEL PASS\n') != 1:
        raise ValueError('Missing launch model completion')
    row = single(serial, 'LAUNCHRESULT', 'generated rejected digest reads before after peak reject_peak tables mappings payload capabilities'.split())
    digest, reads = expected_summary()
    if row != dict(generated=240,rejected=264,digest=digest,reads=reads,before=m2['allocated_frames'],after=m2['allocated_frames'],
                   peak=m2['allocated_frames']+1,reject_peak=m2['allocated_frames'],tables=m2['table_frames'],mappings=m2['virtual_mappings'],payload=0,capabilities=0):
        raise ValueError('Generated execution model, coverage or physical accounting mismatch')
    return row
