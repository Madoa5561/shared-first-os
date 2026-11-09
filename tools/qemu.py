"""Build a UEFI ISO and test it in QEMU TCG. No network, mounts or raw disks."""

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import time
from pathlib import Path
from m5_validation import validate_m5
from m6_validation import validate_m6, benchmark_markdown
from m7_validation import validate_m7, benchmark_markdown as m7_markdown
from m8_validation import validate_m8
from m9_validation import validate_m9
from pack_bytecode import encode

ROOT = Path(__file__).resolve().parent.parent
LOCK = json.loads((ROOT / 'tools/versions.json').read_text())
KINDS = ['usable', 'reserved', 'acpi_reclaimable', 'acpi_nvs', 'bad_memory',
         'bootloader_reclaimable', 'kernel_and_modules', 'framebuffer', 'unknown']


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def checked(command, directory):
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, check=False)
    (directory / 'command.json').write_text(json.dumps({
        'command': command, 'cwd': str(ROOT), 'exit_code': result.returncode,
    }, indent=2) + '\n')
    (directory / 'stdout.log').write_text(result.stdout)
    (directory / 'stderr.log').write_text(result.stderr)
    if result.returncode:
        raise RuntimeError(f'Command failed: {command}; {result.stderr}')
    return result.stdout


def evidence_directory(name):
    directory = ROOT / 'artifacts' / f'{name}-{time.time_ns()}'
    directory.mkdir(parents=True)
    return directory


def make_image(variant):
    directory = evidence_directory(f'image-{variant}')
    limine = ROOT / '.tools/limine'
    for name, expected in LOCK['limine']['files'].items():
        if sha256(limine / name) != expected:
            raise RuntimeError(f'Limine hash mismatch: {name}')
    staging = ROOT / 'build' / variant / 'iso-root'
    (staging / 'boot').mkdir(parents=True, exist_ok=True)
    (staging / 'EFI/BOOT').mkdir(parents=True, exist_ok=True)
    kernel = ROOT / 'target/x86_64-unknown-none/release/shared-first-kernel'
    shutil.copyfile(kernel, staging / 'boot/shared-first-kernel')
    shutil.copyfile(ROOT / 'limine.conf', staging / 'limine.conf')
    spec = json.loads((ROOT / 'examples/bytecode/demo.json').read_text())
    if variant == 'module-alt':
        spec['programs'][0]['instructions'][0][4] = 99
    module = bytearray(encode(spec))
    if variant == 'module-bad-version':
        module[8] = 2
    if variant == 'module-bad-opcode':
        # Corrupt the final program, proving all-or-nothing admission.
        module[-16] = 255
    (staging / 'boot/programs.sfbc').write_bytes(module)
    shutil.copyfile(limine / 'BOOTX64.EFI', staging / 'EFI/BOOT/BOOTX64.EFI')
    shutil.copyfile(limine / 'limine-uefi-cd.bin', staging / 'boot/limine-uefi-cd.bin')
    shutil.copyfile(limine / 'LICENSE', staging / 'boot/LIMINE-LICENSE.txt')
    iso = ROOT / 'build' / variant / 'shared-first.iso'
    # Optical El Torito UEFI image only. No BIOS install or physical disk writes.
    checked(['xorriso', '-as', 'mkisofs', '-R', '-J', '-V', 'SHARED_FIRST',
             '-e', 'boot/limine-uefi-cd.bin', '-no-emul-boot',
             '-efi-boot-part', '--efi-boot-image', '--protective-msdos-label',
             '-o', str(iso), str(staging)], directory)
    (directory / 'hashes.json').write_text(json.dumps({
        'kernel_sha256': sha256(kernel), 'iso_sha256': sha256(iso),
        'limine_commit': LOCK['limine']['commit'],
        'module_sha256': sha256(staging / 'boot/programs.sfbc'),
        'module_bytes': len(module),
    }, indent=2) + '\n')
    print(f'ISO: {iso}; evidence: {directory}')


def validate_serial(serial, exit_code, expect_panic=False, expect_fault=False):
    serial = serial.replace('\r', '')
    if expect_panic and expect_fault:
        raise ValueError('Conflicting failure expectations')
    if exit_code != (35 if expect_panic or expect_fault else 33):
        raise ValueError(f'Unexpected QEMU exit code: {exit_code}')
    if serial.count('Shared-First OS booted\n') != 1:
        raise ValueError('Missing or repeated boot marker')
    if 'BOOT base_revision=3 firmware=2\n' not in serial:
        raise ValueError('Not the requested Limine revision / UEFI64 boot')
    if expect_panic:
        if 'PANIC:' not in serial or 'intentional boot-panic test' not in serial or 'M1 PASS' in serial:
            raise ValueError('Panic path was not observed correctly')
    elif expect_fault:
        if ('PANIC:' not in serial or 'unexpected page fault: cr2=0xffffff0000000000 error=0x0' not in serial
                or 'M2 PASS' in serial or serial.count('M1 PASS\n') != 1):
            raise ValueError('Unexpected-fault path was not observed correctly')
    elif 'PANIC:' in serial or serial.count('M1 PASS\n') != 1:
        raise ValueError('Panic or missing success marker')
    entries = re.findall(r'^MEMMAP index=(\d+) base=(0x[0-9a-f]+) length=(0x[0-9a-f]+) kind=(\d+)$', serial, re.M)
    if not entries:
        raise ValueError('Missing physical memory-map entries')
    totals = [[0, 0] for _ in KINDS]
    for expected_index, (index, base, length, kind) in enumerate(entries):
        base, length, kind = int(base, 16), int(length, 16), int(kind)
        if int(index) != expected_index or length == 0 or base + length > (1 << 64) - 1:
            raise ValueError('Invalid physical memory-map record')
        bucket = min(kind, 8)
        totals[bucket][0] += 1
        totals[bucket][1] += length
    reported = re.findall(r'^MEMKIND name=(\w+) regions=(\d+) bytes=(\d+)$', serial, re.M)
    if [(name, int(count), int(size)) for name, count, size in reported] != [
        (name, *total) for name, total in zip(KINDS, totals)
    ]:
        raise ValueError('Per-kind totals disagree with physical memory-map records')
    summaries = re.findall(r'^MEMSUMMARY entries=(\d+) usable_bytes=(\d+) reserved_bytes=(\d+) described_bytes=(\d+)$', serial, re.M)
    usable = totals[0][1]
    reserved = sum(total[1] for total in totals[1:])
    expected = (len(entries), usable, reserved, usable + reserved)
    if len(summaries) != 1 or tuple(map(int, summaries[0])) != expected or not usable or not reserved:
        raise ValueError('Missing, empty or inconsistent memory summary')
    return dict(zip(['entries', 'usable_bytes', 'reserved_bytes', 'described_bytes'], expected))


def run_qemu(options):
    directory = evidence_directory(f'boot-{options.variant}-{options.memory}M')
    qemu_version = subprocess.check_output(['qemu-system-x86_64', '--version'], text=True).splitlines()[0]
    if not qemu_version.startswith(f"QEMU emulator version {LOCK['qemu']} "):
        raise RuntimeError(f'Expected QEMU {LOCK["qemu"]}; got {qemu_version}')
    code = Path('/usr/share/OVMF/OVMF_CODE_4M.fd')
    template = Path('/usr/share/OVMF/OVMF_VARS_4M.fd')
    # Firmware code stays read-only; only this per-run variable-store copy is writable.
    variables = directory / 'OVMF_VARS_4M.fd'
    shutil.copyfile(template, variables)
    iso = ROOT / 'build' / options.variant / 'shared-first.iso'
    serial_file = directory / 'serial.log'
    command = ['qemu-system-x86_64', '-machine', 'q35', '-accel', 'tcg',
               '-cpu', 'qemu64', '-smp', '1', '-m', str(options.memory),
               '-drive', f'if=pflash,format=raw,readonly=on,file={code}',
               '-drive', f'if=pflash,format=raw,file={variables}',
               '-cdrom', str(iso), '-boot', 'order=d', '-display', 'none',
               '-serial', f'file:{serial_file}', '-monitor', 'none', '-nic', 'none',
               '-no-reboot', '-device', 'isa-debug-exit,iobase=0xf4,iosize=0x04']
    timeout_test = options.action == 'test-timeout'
    if timeout_test:
        command.append('-S')  # Real QEMU, deliberately paused to test its cleanup.
    record = {
        'command': command, 'cwd': str(ROOT), 'timeout_seconds': options.timeout,
        'qemu_version': qemu_version, 'iso_sha256': sha256(iso),
        'firmware_code_sha256': sha256(code), 'firmware_vars_template_sha256': sha256(template),
        'expect_panic': options.expect_panic, 'expect_fault': options.expect_fault, 'expect_timeout': timeout_test,
    }
    started = time.monotonic()
    failure = None
    try:
        with (directory / 'stdout.log').open('wb') as stdout, (directory / 'stderr.log').open('wb') as stderr:
            with subprocess.Popen(command, cwd=ROOT, stdout=stdout, stderr=stderr) as process:
                timed_out = False
                try:
                    process.wait(timeout=options.timeout)
                except subprocess.TimeoutExpired:
                    timed_out = True
                    process.kill()
                    process.wait()
                record.update(exit_code=process.returncode, timed_out=timed_out)
        serial = serial_file.read_text(errors='replace') if serial_file.exists() else ''
        if timeout_test:
            if not timed_out or process.returncode != -9:
                raise ValueError('Timeout did not kill and reap QEMU')
        else:
            if timed_out:
                raise ValueError('QEMU boot timed out')
            record['summary'] = validate_serial(serial, process.returncode, options.expect_panic, options.expect_fault)
            if not options.expect_panic:
                record['m2'] = validate_m2(serial, record['summary']['usable_bytes'], require_success=not options.expect_fault)
                if not options.expect_fault:
                    record['m3'] = validate_m3(serial, record['m2'])
                    record['m4'] = validate_m4(serial, record['m2'], record['m3'])
                    record['m5'] = validate_m5(serial, record['m2'])
                    record['m6'] = validate_m6(serial, record['m2'])
                    (directory / 'm6-benchmark.md').write_text(benchmark_markdown(record['m6']), encoding='utf-8')
                    record['m7'] = validate_m7(serial, record['m2'], record['m6'])
                    (directory / 'm7-benchmark.md').write_text(m7_markdown(record['m7']), encoding='utf-8')
                    record['m8'] = validate_m8(serial, record['m2'])
                    module = (ROOT / 'build' / options.variant / 'iso-root/boot/programs.sfbc').read_bytes()
                    record['m9'] = validate_m9(serial, record['m2'], module, options.variant)
        record['passed'] = True
        # Keep the raw firmware escape sequences in serial.log, but make the
        # command transcript readable. This does not affect validation.
        readable = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', serial)
        (directory / 'serial-readable.log').write_text(readable)
        print(readable)
    except Exception as error:
        record.update(passed=False, error=str(error))
        failure = error
    finally:
        record['elapsed_seconds'] = time.monotonic() - started
        (directory / 'result.json').write_text(json.dumps(record, indent=2) + '\n')
        print(f'Evidence: {directory}; passed={record.get("passed")} exit={record.get("exit_code")}')
    if failure:
        raise failure


def validate_m2(serial, usable_bytes, require_success=True):
    serial = serial.replace('\r', '')
    def record(pattern):
        matches = re.findall(pattern, serial, re.M)
        if len(matches) != 1:
            raise ValueError(f'Missing or duplicate M2 record: {pattern}')
        return tuple(int(value, 0) for value in matches[0])
    if serial.count('M2 PASS\n') != (1 if require_success else 0):
        raise ValueError('Missing or repeated M2 success marker')
    total_start, metadata, limit = record(r'^M2START total_frames=(\d+) bitmap_bytes=(\d+) limit_bytes=(\d+)$')
    root, initial_tables, initial_mappings = record(r'^M2PAGING root=(0x[0-9a-f]+) table_frames=(\d+) mappings=(\d+) wp=1 nx=1$')
    stack, bottom, top = record(r'^M2STACK rsp=(0x[0-9a-f]+) bottom=(0x[0-9a-f]+) top=(0x[0-9a-f]+) guards=2$')
    total, allocated, free, tables, stack_frames, bitmap, scratch, mappings = record(
        r'^M2FRAMES total=(\d+) allocated=(\d+) free=(\d+) table_frames=(\d+) stack_frames=(\d+) payload_frames=0 bitmap_bytes=(\d+) scratch_bytes=(\d+) mappings=(\d+)$')
    reclaimed, faults = record(r'^M2CHECK exhausted_reclaimed=(\d+) reserved_excluded=1 map_unmap=1 tlb=1 readonly=1 nx=1 guards=2 wx_aliases=1 faults=(\d+)$')
    actual_faults = [(int(address, 16), int(error)) for address, error in re.findall(
        r'^M2FAULT address=(0x[0-9a-f]+) error=(\d+) recovered=1$', serial, re.M)]
    if not (0 < total == total_start <= usable_bytes // 4096 and total == allocated + free
            and allocated == tables + stack_frames and tables == initial_tables
            and mappings == initial_mappings and reclaimed == free and free > 0):
        raise ValueError('Inconsistent M2 frame/table/reclamation accounting')
    if not (root > 0 and root % 4096 == 0 and bottom <= stack < top
            and top - bottom == stack_frames * 4096 and stack_frames == 16
            and metadata == bitmap == 65536 and scratch == 32768 and limit == 1073741824):
        raise ValueError('Invalid M2 paging/stack/metadata configuration')
    if (faults != 7 or len(actual_faults) != 7
            or sorted(error for _, error in actual_faults) != [0, 0, 0, 3, 3, 3, 17]
            or (bottom - 4096, 0) not in actual_faults or (top, 0) not in actual_faults
            or any((0xffff_ff00_0000_0000, error) not in actual_faults for error in [0, 3, 17])):
        raise ValueError('Missing hardware permission/guard/TLB fault evidence')
    return {'total_frames': total, 'allocated_frames': allocated, 'free_frames': free,
            'table_frames': tables, 'stack_frames': stack_frames, 'payload_frames': 0,
            'bitmap_bytes': bitmap, 'test_scratch_bytes': scratch, 'virtual_mappings': mappings,
            'reclaimed_frames': reclaimed, 'recovered_page_faults': faults}


def validate_m3(serial, m2):
    serial = serial.replace('\r', '')
    def record(pattern):
        matches = re.findall(pattern, serial, re.M)
        if len(matches) != 1:
            raise ValueError(f'Missing or duplicate M3 record: {pattern}')
        return tuple(int(value, 0) for value in matches[0])
    if serial.count('M3 PASS\n') != 1:
        raise ValueError('Missing or repeated M3 success marker')
    length, payload, caps, views, metadata, allocated, tables = record(
        r'^M3OBJECT length=(\d+) payload_frames=(\d+) capabilities=(\d+) mapping_pages=(\d+) metadata_bytes=(\d+) allocated=(\d+) table_frames=(\d+)$')
    before, after, tables_before, tables_after, maps_before, maps_after = record(
        r'^M3FRAMES before=(\d+) after=(\d+) tables_before=(\d+) tables_after=(\d+) mappings_before=(\d+) mappings_after=(\d+) payload_frames=0 capabilities=0 objects=0$')
    faults, = record(r'^M3CHECK shared_frames=1 zero_tail=1 rights=1 stale=1 revoke=1 release=1 create_rollback=1 map_rollback=1 drop_cleanup=1 hhdm_readonly=1 faults=(\d+)$')
    if not (length == 4097 and payload == 2 and caps == 2 and views == 4 and metadata > 0
            and before == after == m2['allocated_frames']
            and tables_before == tables_after == m2['table_frames']
            and maps_before == maps_after == m2['virtual_mappings']
            and tables > tables_before and allocated == before + payload + tables - tables_before):
        raise ValueError('Inconsistent M3 physical/metadata/virtual accounting or leaked frames')
    pages = [tuple(int(value, 0) for value in row) for row in re.findall(
        r'^M3PAGE index=(\d+) physical=(0x[0-9a-f]+) owner_va=(0x[0-9a-f]+) reader_va=(0x[0-9a-f]+) hhdm=(0x[0-9a-f]+) rights=ro_nx$', serial, re.M)]
    if len(pages) != 2 or len({row[1] for row in pages}) != 2:
        raise ValueError('Missing, duplicate or aliased M3 payload frames')
    owner, reader = 0xffff_fd00_0000_1000, 0xffff_fd00_0020_1000
    for expected, (index, frame, first, second, hhdm) in enumerate(pages):
        if not (index == expected and frame > 0 and frame % 4096 == 0
                and first == owner + index * 4096 and second == reader + index * 4096
                and hhdm % 4096 == 0 and hhdm - frame == pages[0][4] - pages[0][1]
                and hhdm >= 0xffff_8000_0000_0000):
            raise ValueError('Invalid M3 shared page/alias record')
    actual_faults = [(int(address, 16), int(error)) for address, error in re.findall(
        r'^M3FAULT address=(0x[0-9a-f]+) error=(\d+) recovered=1$', serial, re.M)]
    expected_faults = [(row[4], 3) for row in pages] + [
        (owner, 3), (reader, 3), (reader, 17),
        (owner, 0), (reader, 0), (owner, 0), (reader, 0)]
    if faults != 9 or actual_faults != expected_faults:
        raise ValueError('Missing M3 hardware RO/NX/revoke/release fault evidence')
    return {'object_bytes': length, 'payload_frames': payload, 'capabilities': caps,
            'object_mapping_pages': views, 'metadata_bytes': metadata,
            'live_table_frames': tables, 'live_allocated_frames': allocated,
            'final_allocated_frames': after, 'final_payload_frames': 0,
            'recovered_page_faults': faults, 'physical_frames': [row[1] for row in pages],
            'hhdm_offset': pages[0][4] - pages[0][1]}


def validate_m4(serial, m2, m3):
    serial = serial.replace('\r', '')
    def rows(pattern):
        return [tuple(int(value, 0) for value in row) for row in re.findall(pattern, serial, re.M)]
    def record(pattern):
        found = rows(pattern)
        if len(found) != 1:
            raise ValueError(f'Missing or duplicate M4 record: {pattern}')
        return found[0]
    if serial.count('M4 PASS\n') != 1:
        raise ValueError('Missing or repeated M4 success marker')
    size, pages, root0, root1, original, payload, views, metadata, allocated, tables = record(
        r'^M4SHARED bytes=(\d+) pages=(\d+) task0_root=(0x[0-9a-f]+) task1_root=(0x[0-9a-f]+) first_frame=(0x[0-9a-f]+) payload_frames=(\d+) mapping_pages=(\d+) metadata_bytes=(\d+) allocated=(\d+) table_frames=(\d+) task_frames=5$')
    old, private, copied, unchanged, cow_payload, cow_allocated = record(
        r'^M4PRIVATE original=(0x[0-9a-f]+) private=(0x[0-9a-f]+) copied_pages=(\d+) unchanged_pages=(\d+) payload_frames=(\d+) allocated=(\d+)$')
    before, after, tables_before, tables_after, maps_before, maps_after = record(
        r'^M4FRAMES before=(\d+) after=(\d+) tables_before=(\d+) tables_after=(\d+) mappings_before=(\d+) mappings_after=(\d+) payload_frames=0 private_frames=0 task_frames=0 capabilities=0$')
    entries, calls, denied, cows, killed, exited, shared_count, unchanged_count = record(
        r'^M4CHECK entries=(\d+) syscalls=(\d+) denied=(\d+) cow_faults=(\d+) killed=(\d+) exited=(\d+) shared_pages=(\d+) unchanged_pages=(\d+) revoke=1 cleanup=1 ring3=1$')
    if not (size == 4194304 and pages == payload == shared_count == 1024 and views == 2048
            and metadata > 0 and root0 != root1 and len({root0, root1, original, private}) == 4
            and all(value > 0 and value % 4096 == 0 for value in [root0, root1, original, private])
            and before == after == m2['allocated_frames']
            and tables_before == tables_after == m2['table_frames']
            and maps_before == maps_after == m2['virtual_mappings']
            and tables > tables_before and allocated == before + payload + 5 + tables - tables_before
            and copied == cows == 1 and old == original and unchanged == unchanged_count == 1023
            and cow_payload == 1025 and cow_allocated == allocated + 1):
        raise ValueError('Invalid M4 address-space, physical sharing, COW or reclamation accounting')
    tasks = rows(r'^M4TASK task=(\d+) root=(0x[0-9a-f]+) code=(0x[0-9a-f]+) stack=(0x[0-9a-f]+) attack=(\d+)$')
    if len(tasks) != 14 or [(row[0], row[4]) for row in tasks] != [(0, 0), (1, 0)] + [(2, case) for case in range(12)]:
        raise ValueError('Missing task/attack launches')
    if tasks[0][1] != root0 or tasks[1][1] != root1:
        raise ValueError('Shared roots differ from task launches')
    for task, root, code, stack, _ in tasks:
        if (len({root, code, stack}) != 3 or any(value == 0 or value % 4096 for value in [root, code, stack])
                or (task == 2 and root in [root0, root1])):
            raise ValueError('Invalid task-private pages or root')
    total = size * 0x5a
    users = rows(r'^M4USER task=(\d+) phase=(\d+) checksum=(\d+) cs=3 root=(0x[0-9a-f]+)$')
    if users != [(0, 0, total, root0), (1, 0, total, root1),
                 (0, 1, total + 155, root0), (1, 1, total, root1), (0, 2, 0, root0)]:
        raise ValueError('Missing actual ring-3 read checkpoints or unchanged reader evidence')
    if rows(r'^M4COW task=(\d+) address=(0x[0-9a-f]+) error=7 cs=3 copies=(\d+)$') != [(0, 0x4000007b, 1)]:
        raise ValueError('Missing single hardware COW write fault')
    if serial.count('M4COWFAIL task=2 allocation_denied=1\n') != 1:
        raise ValueError('Missing COW allocation-failure path')
    if serial.count('M4CREATE task=2 bytes=4097 fill=51\n') != 1:
        raise ValueError('Missing user-created object lifecycle')
    faults = rows(r'^M4FAULT task=(\d+) vector=(\d+) error=(\d+) address=(0x[0-9a-f]+) cs=3 killed=1$')
    expected = [(2, 14, 7, 0x40000000), (2, 14, 21, 0x40000000),
                (2, 14, 5, 0xffffffff80000000), (2, 14, 5, m3['hhdm_offset'] + original),
                (2, 14, 4, 0x900000), (2, 13, 0, 0), (2, 13, 1034, 0),
                (2, 14, 7, 0x400000), (2, 14, 4, 0x7ff000), (2, 7, 0, 0),
                (2, 14, 7, 0x40000000), (0, 14, 4, 0x40000000), (1, 14, 4, 0x40000000)]
    if faults != expected or killed != 13:
        raise ValueError('Missing user protection, COW failure, or revocation faults')
    attacks = re.findall(r'^M4ATTACK case=(\d+) reclaimed=1$', serial, re.M)
    if list(map(int, attacks)) != list(range(12)):
        raise ValueError('Missing attack reclamation evidence')
    denials = re.findall(r'^M4DENY task=(\d+) syscall=(\d+) reason=(.+)$', serial, re.M)
    if (denied != len(denials) or denied != 87 or entries != calls + cows + killed
            or calls != 128 or entries != 142 or exited != 1
            or rows(r'^M4EXIT task=(\d+) status=(\d+) cs=3$') != [(2, 0)]):
        raise ValueError('Inconsistent syscall/trap/termination accounting')
    for task, expected_count in [(0, 5), (1, 7), (2, 75)]:
        selected = [(int(number), reason) for who, number, reason in denials if int(who) == task]
        if (len(selected) != expected_count or (1, 'StaleHandle') not in selected
                or (1, 'InvalidAddress') not in selected or (999, 'Denied') not in selected):
            raise ValueError('Missing task-local syscall authority checks')
    if sum(who == '2' and number == '9' and reason == 'InvalidSize' for who, number, reason in denials) != 2:
        raise ValueError('Missing create argument validation')
    return {'shared_bytes': size, 'shared_physical_frames': payload, 'user_mapping_pages': views,
            'metadata_bytes': metadata, 'shared_allocated_frames': allocated, 'shared_table_frames': tables,
            'cow_private_frames': copied, 'cow_payload_frames': cow_payload, 'cow_allocated_frames': cow_allocated,
            'unchanged_pages': unchanged, 'task_roots': [root0, root1], 'original_frame': original,
            'private_frame': private, 'syscalls': calls, 'denied_syscalls': denied, 'user_entries': entries,
            'terminated_by_fault': killed, 'normal_exits': exited, 'attack_cases': len(attacks),
            'final_allocated_frames': after, 'final_payload_frames': 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['image', 'test', 'test-timeout'])
    parser.add_argument('--variant', choices=['normal', 'panic', 'fault', 'module-alt', 'module-bad-version', 'module-bad-opcode'], default='normal')
    parser.add_argument('--memory', type=int, choices=[128, 256], default=128)
    parser.add_argument('--timeout', type=float, default=90)
    expected = parser.add_mutually_exclusive_group()
    expected.add_argument('--expect-panic', action='store_true')
    expected.add_argument('--expect-fault', action='store_true')
    options = parser.parse_args()
    if not 0 < options.timeout <= 180:
        parser.error('--timeout must be greater than 0 and at most 180 seconds')
    if options.action == 'image':
        make_image(options.variant)
    else:
        run_qemu(options)


if __name__ == '__main__':
    main()
