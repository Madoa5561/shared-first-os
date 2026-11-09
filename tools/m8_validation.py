"""Strict M8 dispatch, fuel, sharing and cleanup evidence validation."""
from m6_validation import records

FIELDS = 'scenario task result value before after'.split()
# Independent expected execution of the documented built-in programs.
# result: 0=quantum, 1=yield, 2=halt, 3=fuel, 4=authority, 5=bounds.
EXPECTED = [
    (0, 0, 0, 0, 5, 3), (0, 1, 2, 42, 2, 0),
    (0, 2, 1, 0, 3, 2), (0, 3, 5, 0, 2, 0),
    (0, 0, 0, 0, 3, 1), (0, 2, 1, 0, 2, 0),
    (0, 0, 3, 0, 1, 0), (0, 2, 3, 0, 0, 0),
    (1, 0, 0, 0, 5, 3), (1, 1, 0, 0, 10000, 5902),
    (1, 2, 0, 0, 10000, 5902), (1, 3, 4, 0, 10000, 5902),
    (1, 0, 0, 0, 3, 1), (1, 1, 4, 0, 5902, 1805),
    (1, 2, 4, 0, 5902, 1805), (1, 0, 3, 0, 1, 0),
    (2, 1, 2, 0, 1, 0),
]


def single(serial, prefix, fields):
    rows = records(serial, prefix, set(fields))
    if len(rows) != 1:
        raise ValueError(f'Missing or duplicate {prefix}')
    return rows[0]


def validate_m8(serial, m2):
    serial = serial.replace('\r', '')
    if serial.count('M8 PASS\n') != 1:
        raise ValueError('Missing or duplicate M8 completion')
    config = single(serial, 'M8CONFIG', 'capacity quantum max_quantum max_batch_fuel scheduler_bytes host_bytes'.split())
    if (any(config[key] != value for key, value in dict(capacity=4, quantum=2, max_quantum=64, max_batch_fuel=2097152).items())
            or not 4 * 136 <= config['scheduler_bytes'] <= 4096
            or config['host_bytes'] != 72):
        raise ValueError('Invalid M8 configuration/accounting')
    dispatches = records(serial, 'M8DISPATCH', set(FIELDS))
    if [tuple(row[key] for key in FIELDS) for row in dispatches] != EXPECTED:
        raise ValueError('M8 scheduling order, outcomes or fuel mismatch')
    shared = single(serial, 'M8SHARED', 'frame1 frame2 payload capabilities mappings sum1 sum2'.split())
    if (shared['frame1'] != shared['frame2'] or shared['frame1'] <= 0 or shared['frame1'] % 4096
            or any(shared[key] != value for key, value in dict(payload=1, capabilities=3, mappings=0, sum1=28672, sum2=28672).items())):
        raise ValueError('M8 physical sharing, checksum or authority mismatch')
    quota = single(serial, 'M8QUOTAS', 'zero overflow aggregate capacity quantum no_refund no_reuse cancel shutdown'.split())
    if any(value != 1 for value in quota.values()):
        raise ValueError('M8 quota/termination check failed')
    frames = single(serial, 'M8FRAMES', 'before after tables_before tables_after mappings_before mappings_after payload capabilities'.split())
    expected_frames = dict(before=m2['allocated_frames'], after=m2['allocated_frames'],
                           tables_before=m2['table_frames'], tables_after=m2['table_frames'],
                           mappings_before=m2['virtual_mappings'], mappings_after=m2['virtual_mappings'],
                           payload=0, capabilities=0)
    if frames != expected_frames:
        raise ValueError('M8 leaked physical frames, mappings or capabilities')
    check = single(serial, 'M8CHECK', 'dispatches fairness fault_isolation shared revocation cleanup'.split())
    if check != dict(dispatches=17, fairness=1, fault_isolation=1, shared=1, revocation=1, cleanup=1):
        raise ValueError('Incomplete M8 checks')
    return dict(config=config, dispatches=dispatches, shared=shared, quotas=quota, frames=frames, check=check)
