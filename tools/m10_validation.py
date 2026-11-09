"""Scoped lifecycle evidence against real object/frame accounting."""
from m6_validation import records
from m8_validation import single


def validate_m10(serial, m2):
    serial = serial.replace('\r', '')
    if serial.count('M10 PASS\n') != 1:
        raise ValueError('Missing or duplicate M10 completion')
    fields = set('case turns stops calls before after peak tables mappings payload capabilities code_bytes'.split())
    cases = records(serial, 'M10CASE', fields)
    expected = [dict(case=case, turns=int(case != 0), stops=3, calls=int(case == 3),
                     before=m2['allocated_frames'], after=m2['allocated_frames'], peak=m2['allocated_frames']+1,
                     tables=m2['table_frames'], mappings=m2['virtual_mappings'], payload=0, capabilities=0, code_bytes=0)
                for case in range(4)]
    if cases != expected:
        raise ValueError('M10 missing lifecycle case, duplicate stop or leaked resources')
    check = single(serial, 'M10CHECK', 'cases session_bytes host_bound drop_cleanup once code_lifetime'.split())
    if check != dict(cases=4, session_bytes=616, host_bound=1, drop_cleanup=1, once=1, code_lifetime=1):
        raise ValueError('M10 scope/accounting mismatch')
    return dict(cases=cases, check=check)
