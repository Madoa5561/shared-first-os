"""Validator tests are synthetic; only qemu.py test is actual boot evidence."""
import unittest
from qemu import KINDS, validate_serial, validate_m2, validate_m3, validate_m4


def transcript():
    lines = ['Shared-First OS booted', 'BOOT base_revision=3 firmware=2',
             'MEMMAP index=0 base=0x1000 length=0x1000 kind=0',
             'MEMMAP index=1 base=0x2000 length=0x1000 kind=1']
    for index, name in enumerate(KINDS):
        lines.append(f'MEMKIND name={name} regions={int(index < 2)} bytes={4096 if index < 2 else 0}')
    lines += ['MEMSUMMARY entries=2 usable_bytes=4096 reserved_bytes=4096 described_bytes=8192', 'M1 PASS']
    return '\n'.join(lines) + '\n'


class ValidatorTests(unittest.TestCase):
    def test_valid_transcript(self):
        self.assertEqual(validate_serial(transcript(), 33)['usable_bytes'], 4096)

    def test_rejects_wrong_exit_code_even_with_marker(self):
        for code in [0, 1, 35, -9]:
            with self.subTest(code=code), self.assertRaises(ValueError):
                validate_serial(transcript(), code)

    def test_rejects_marker_only(self):
        with self.assertRaises(ValueError):
            validate_serial('Shared-First OS booted\n', 33)

    def test_rejects_bad_accounting(self):
        with self.assertRaises(ValueError):
            validate_serial(transcript().replace('reserved_bytes=4096', 'reserved_bytes=0'), 33)

    def test_rejects_panic_after_success(self):
        with self.assertRaises(ValueError):
            validate_serial(transcript() + 'PANIC: late failure\n', 33)

    def test_rejects_missing_or_repeated_success(self):
        for text in [transcript().replace('M1 PASS\n', ''), transcript() + 'M1 PASS\n']:
            with self.assertRaises(ValueError):
                validate_serial(text, 33)

    def test_panic_requires_explicit_expectation(self):
        text = transcript().replace('M1 PASS', 'PANIC: intentional boot-panic test')
        with self.assertRaises(ValueError):
            validate_serial(text, 35)
        validate_serial(text, 35, expect_panic=True)


def m2_transcript():
    return '\n'.join([
        'M2START total_frames=64 bitmap_bytes=65536 limit_bytes=1073741824',
        'M2PAGING root=0x1000 table_frames=4 mappings=90 wp=1 nx=1',
        'M2STACK rsp=0xfffffe8000010000 bottom=0xfffffe8000001000 top=0xfffffe8000011000 guards=2',
        'M2FAULT address=0xfffffe8000000000 error=0 recovered=1',
        'M2FAULT address=0xfffffe8000011000 error=0 recovered=1',
        'M2FAULT address=0xffffffff80001000 error=3 recovered=1',
        'M2FAULT address=0xffff800000101000 error=3 recovered=1',
        'M2FAULT address=0xffffff0000000000 error=0 recovered=1',
        'M2FAULT address=0xffffff0000000000 error=3 recovered=1',
        'M2FAULT address=0xffffff0000000000 error=17 recovered=1',
        'M2FRAMES total=64 allocated=20 free=44 table_frames=4 stack_frames=16 payload_frames=0 bitmap_bytes=65536 scratch_bytes=32768 mappings=90',
        'M2CHECK exhausted_reclaimed=44 reserved_excluded=1 map_unmap=1 tlb=1 readonly=1 nx=1 guards=2 wx_aliases=1 faults=7',
        'M2 PASS', ''])


class M2ValidatorTests(unittest.TestCase):
    def test_unexpected_fault_requires_exact_diagnostic(self):
        text = transcript() + 'PANIC: unexpected page fault: cr2=0xffffff0000000000 error=0x0 rip=0xffffffff80001000\n'
        validate_serial(text, 35, expect_fault=True)
        with self.assertRaises(ValueError):
            validate_serial(text.replace('error=0x0', 'error=0x3'), 35, expect_fault=True)
        with self.assertRaises(ValueError):
            validate_serial(text, 35)

    def test_accepts_complete_m2_evidence(self):
        self.assertEqual(validate_m2(m2_transcript(), 64 * 4096)['recovered_page_faults'], 7)

    def test_m1_success_is_not_m2_success(self):
        with self.assertRaises(ValueError):
            validate_m2(transcript(), 64 * 4096)

    def test_rejects_leaked_frames_or_missing_hardware_faults(self):
        for old, new in [('free=44', 'free=43'), ('exhausted_reclaimed=44', 'exhausted_reclaimed=43'),
                         ('error=17', 'error=3'), ('wx_aliases=1', 'wx_aliases=0'),
                         ('M2 PASS', '')]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                validate_m2(m2_transcript().replace(old, new), 64 * 4096)


def m3_transcript():
    return '\n'.join([
        'M3OBJECT length=4097 payload_frames=2 capabilities=2 mapping_pages=4 metadata_bytes=5000 allocated=26 table_frames=8',
        'M3PAGE index=0 physical=0x8000 owner_va=0xfffffd0000001000 reader_va=0xfffffd0000201000 hhdm=0xffff800000008000 rights=ro_nx',
        'M3FAULT address=0xffff800000008000 error=3 recovered=1',
        'M3PAGE index=1 physical=0x9000 owner_va=0xfffffd0000002000 reader_va=0xfffffd0000202000 hhdm=0xffff800000009000 rights=ro_nx',
        'M3FAULT address=0xffff800000009000 error=3 recovered=1',
        'M3FAULT address=0xfffffd0000001000 error=3 recovered=1',
        'M3FAULT address=0xfffffd0000201000 error=3 recovered=1',
        'M3FAULT address=0xfffffd0000201000 error=17 recovered=1',
        'M3FAULT address=0xfffffd0000001000 error=0 recovered=1',
        'M3FAULT address=0xfffffd0000201000 error=0 recovered=1',
        'M3FAULT address=0xfffffd0000001000 error=0 recovered=1',
        'M3FAULT address=0xfffffd0000201000 error=0 recovered=1',
        'M3FRAMES before=20 after=20 tables_before=4 tables_after=4 mappings_before=90 mappings_after=90 payload_frames=0 capabilities=0 objects=0',
        'M3CHECK shared_frames=1 zero_tail=1 rights=1 stale=1 revoke=1 release=1 create_rollback=1 map_rollback=1 drop_cleanup=1 hhdm_readonly=1 faults=9',
        'M3 PASS', ''])


class M3ValidatorTests(unittest.TestCase):
    def setUp(self):
        self.m2 = validate_m2(m2_transcript(), 64 * 4096)

    def test_accepts_full_object_and_hardware_evidence(self):
        result = validate_m3(m3_transcript(), self.m2)
        self.assertEqual(result['payload_frames'], 2)
        self.assertEqual(result['object_mapping_pages'], 4)

    def test_m2_or_marker_alone_is_not_m3_success(self):
        for text in [m2_transcript(), m2_transcript() + 'M3 PASS\n', m3_transcript() + 'M3 PASS\n']:
            with self.assertRaises(ValueError):
                validate_m3(text, self.m2)

    def test_rejects_leaks_wrong_permissions_and_accounting(self):
        for old, new in [('after=20', 'after=21'), ('payload_frames=2', 'payload_frames=4'),
                         ('allocated=26', 'allocated=27'), ('mapping_pages=4', 'mapping_pages=2'),
                         ('metadata_bytes=5000', 'metadata_bytes=0'), ('rights=ro_nx', 'rights=rw_nx'),
                         ('revoke=1', 'revoke=0'), ('hhdm_readonly=1', 'hhdm_readonly=0'),
                         ('M3 PASS', '')]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                validate_m3(m3_transcript().replace(old, new), self.m2)


    def test_rejects_missing_or_wrong_faults_and_backing(self):
        for old, new in [('error=17', 'error=3'), ('error=0', 'error=3'),
                         ('physical=0x9000', 'physical=0x8000'), ('physical=0x8000', 'physical=0x8001'),
                         ('hhdm=0xffff800000009000', 'hhdm=0xffff80000000a000')]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                validate_m3(m3_transcript().replace(old, new), self.m2)


def m4_transcript():
    lines = [
        'M4TASK task=0 root=0x500000 code=0x501000 stack=0x502000 attack=0',
        'M4TASK task=1 root=0x510000 code=0x511000 stack=0x512000 attack=0',
        'M4SHARED bytes=4194304 pages=1024 task0_root=0x500000 task1_root=0x510000 first_frame=0x10000 payload_frames=1024 mapping_pages=2048 metadata_bytes=331584 allocated=1065 table_frames=20 task_frames=5',
        'M4COW task=0 address=0x4000007b error=7 cs=3 copies=1',
        'M4PRIVATE original=0x10000 private=0x530000 copied_pages=1 unchanged_pages=1023 payload_frames=1025 allocated=1066',
        'M4COWFAIL task=2 allocation_denied=1',
        'M4CREATE task=2 bytes=4097 fill=51',
        'M4FRAMES before=20 after=20 tables_before=4 tables_after=4 mappings_before=90 mappings_after=90 payload_frames=0 private_frames=0 task_frames=0 capabilities=0',
        'M4CHECK entries=142 syscalls=128 denied=87 cow_faults=1 killed=13 exited=1 shared_pages=1024 unchanged_pages=1023 revoke=1 cleanup=1 ring3=1',
        'M4EXIT task=2 status=0 cs=3',
    ]
    for task, phase, checksum, root in [(0, 0, 377487360, 0x500000), (1, 0, 377487360, 0x510000),
                                       (0, 1, 377487515, 0x500000), (1, 1, 377487360, 0x510000),
                                       (0, 2, 0, 0x500000)]:
        lines.append(f'M4USER task={task} phase={phase} checksum={checksum} cs=3 root={root:#x}')
    for case in range(12):
        lines.append(f'M4TASK task=2 root=0x520000 code=0x521000 stack=0x522000 attack={case}')
        lines.append(f'M4ATTACK case={case} reclaimed=1')
    fault_cases = [(2, 14, 7, 0x40000000), (2, 14, 21, 0x40000000),
                   (2, 14, 5, 0xffffffff80000000), (2, 14, 5, 0xffff800000010000),
                   (2, 14, 4, 0x900000), (2, 13, 0, 0), (2, 13, 1034, 0),
                   (2, 14, 7, 0x400000), (2, 14, 4, 0x7ff000), (2, 7, 0, 0),
                   (2, 14, 7, 0x40000000), (0, 14, 4, 0x40000000), (1, 14, 4, 0x40000000)]
    for task, vector, error, address in fault_cases:
        lines.append(f'M4FAULT task={task} vector={vector} error={error} address={address:#x} cs=3 killed=1')
    base_denials = [(999, 'Denied'), (1, 'StaleHandle'), (1, 'Denied'), (1, 'InvalidAddress')]
    for task, repetitions in [(0, 1), (1, 1), (2, 12)]:
        sequence = base_denials + ([(1, 'Denied'), (5, 'Denied')] if task else [])
        for number, reason in sequence * repetitions + [(1, 'StaleHandle')]:
            lines.append(f'M4DENY task={task} syscall={number} reason={reason}')
    lines += ['M4DENY task=2 syscall=9 reason=InvalidSize'] * 2
    return '\n'.join(lines + ['M4 PASS', ''])


class M4ValidatorTests(unittest.TestCase):
    def setUp(self):
        self.m2 = validate_m2(m2_transcript(), 64 * 4096)
        self.m3 = validate_m3(m3_transcript(), self.m2)

    def test_accepts_ring3_sharing_cow_and_reclamation_evidence(self):
        result = validate_m4(m4_transcript(), self.m2, self.m3)
        self.assertEqual(result['shared_physical_frames'], 1024)
        self.assertEqual(result['cow_private_frames'], 1)
        self.assertEqual(result['terminated_by_fault'], 13)

    def test_m3_or_marker_alone_cannot_pass_m4(self):
        for text in [m3_transcript(), m3_transcript() + 'M4 PASS\n', m4_transcript() + 'M4 PASS\n']:
            with self.assertRaises(ValueError):
                validate_m4(text, self.m2, self.m3)

    def test_rejects_fake_isolation_or_changed_reader(self):
        for old, new in [('task1_root=0x510000', 'task1_root=0x500000'),
                         ('cs=3', 'cs=0'), ('task=1 phase=1 checksum=377487360', 'task=1 phase=1 checksum=377487515'),
                         ('private=0x530000', 'private=0x10000'), ('copied_pages=1', 'copied_pages=1024'),
                         ('mapping_pages=2048', 'mapping_pages=1024')]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                validate_m4(m4_transcript().replace(old, new), self.m2, self.m3)

    def test_rejects_missing_attacks_syscall_checks_and_cow_failure(self):
        for old, new in [('error=21', 'error=17'), ('case=11 reclaimed=1', 'case=11 reclaimed=0'),
                         ('allocation_denied=1', 'allocation_denied=0'), ('status=0 cs=3', 'status=1 cs=3'),
                         ('reason=StaleHandle', 'reason=Unknown'), ('denied=87', 'denied=86'),
                         ('M4CREATE task=2 bytes=4097 fill=51', ''), ('syscalls=128', 'syscalls=127')]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                validate_m4(m4_transcript().replace(old, new), self.m2, self.m3)

    def test_rejects_physical_leaks_and_double_counted_mappings(self):
        for old, new in [('after=20', 'after=21'), ('allocated=1065', 'allocated=1066'),
                         ('payload_frames=1024', 'payload_frames=2048'), ('tables_after=4', 'tables_after=5'),
                         ('private_frames=0', 'private_frames=1')]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                validate_m4(m4_transcript().replace(old, new), self.m2, self.m3)
