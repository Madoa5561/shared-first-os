# Position-independent ring-3 program, copied into a private user RX frame.
# int 0x80 ABI: rax number; rdi/rsi/rdx values; rax result (all-ones = error).
# r12 role, r13 task-local handle, r14 attack case, r15 attack address.
.section .rodata.user_program,"a"
.balign 16
.global m4_user_start
m4_user_start:
    std                         # Kernel entry must clear DF before copying state.
    mov eax, 6                  # WHOAMI ignores a forged caller argument.
    mov edi, 999
    int 0x80
    cld
    cmp rax, r12
    jne user_fail
    mov eax, 999                # Unknown syscall.
    int 0x80
    cmp rax, -1
    jne user_fail
    mov eax, 1                  # Map original READ or COW (still RO until fault).
    mov rdi, r13
    mov esi, 0x40000000
    mov edx, 1
    test r12, r12
    jz user_cow_map
    cmp r14, 11
    jne 1f
user_cow_map:
    mov edx, 64
1:  int 0x80
    test rax, rax
    jnz user_fail
    mov eax, 1                  # Forged generation.
    mov rdi, r13
    xor rdi, 0x10000
    int 0x80
    cmp rax, -1
    jne user_fail
    mov eax, 1                  # WRITE cannot mutate immutable backing.
    mov rdi, r13
    mov edx, 16
    int 0x80
    cmp rax, -1
    jne user_fail
    mov eax, 1                  # Cannot map over supervisor addresses.
    mov rsi, 0xffffffff80000000
    mov edx, 1
    int 0x80
    cmp rax, -1
    jne user_fail
    test r12, r12
    jz 2f
    mov eax, 1                  # Reader cannot obtain a COW mapping.
    mov esi, 0x40000000
    mov edx, 64
    int 0x80
    cmp rax, -1
    jne user_fail
    mov eax, 5                  # Reader cannot delegate OWNER to task 0.
    xor esi, esi
    mov edx, 79
    int 0x80
    cmp rax, -1
    jne user_fail
2:  cmp r12, 2
    je user_attack
    call user_sum
    mov eax, 2                  # Phase 0: each task read the original 4 MiB.
    xor edi, edi
    int 0x80
    test r12, r12
    jnz 3f
    cmp byte ptr [0x900000], 0x42
    jne user_fail
    mov byte ptr [0x4000007b], 0xa7  # Exactly one hardware COW fault.
    mov byte ptr [0x4000007c], 0xa8  # Already private, no second copy.
3:  call user_sum
    mov eax, 2
    mov edi, 1                  # Phase 1: private writer / unchanged reader.
    int 0x80
    test r12, r12
    jnz user_stale
    mov eax, 5                  # A legitimate attenuated grant through syscall.
    mov rdi, r13
    mov esi, 1
    mov edx, 1
    int 0x80
    cmp rax, -1
    je user_fail
    mov eax, 4                  # Object-wide revoke, including private COW pages.
    mov rdi, r13
    int 0x80
    test rax, rax
    jnz user_fail
    mov eax, 2
    mov edi, 2
    xor esi, esi
    int 0x80
user_stale:
    mov eax, 1
    mov rdi, r13
    mov esi, 0x40000000
    mov edx, 1
    int 0x80
    cmp rax, -1
    jne user_fail
    mov al, byte ptr [0x40000000]  # Must terminate: revoked PTE, error 4.
    jmp user_fail

user_sum:
    mov ebx, 0x40000000
    mov ecx, 0x400000
    xor esi, esi
4:  movzx eax, byte ptr [rbx]
    add rsi, rax
    inc rbx
    dec ecx
    jnz 4b
    ret

user_attack:
    cmp r14, 0
    je user_exit
    cmp r14, 1
    je attack_write
    cmp r14, 2
    je attack_execute
    cmp r14, 3
    je attack_pointer
    cmp r14, 4
    je attack_pointer
    cmp r14, 5
    je attack_peer
    cmp r14, 6
    je attack_io
    cmp r14, 7
    je attack_gate
    cmp r14, 8
    je attack_code
    cmp r14, 9
    je attack_guard
    cmp r14, 10
    je attack_fpu
    cmp r14, 11
    je attack_write
    jmp user_fail
attack_write:
    mov byte ptr [0x40000000], 1
    jmp user_fail
attack_execute:
    mov eax, 0x40000000
    jmp rax
attack_pointer:
    mov al, [r15]
    jmp user_fail
attack_peer:
    mov al, byte ptr [0x900000]
    jmp user_fail
attack_io:
    mov dx, 0xf4
    xor eax, eax
    out dx, al
    jmp user_fail
attack_gate:
    int 0x81
    jmp user_fail
attack_code:
    mov byte ptr [0x400000], 0
    jmp user_fail
attack_guard:
    mov al, byte ptr [0x7ff000]
    jmp user_fail
attack_fpu:
    fninit
    jmp user_fail
user_exit:
    mov eax, 7                  # Explicit unmap, then remap, then release.
    mov rdi, r13
    mov esi, 0x40000000
    int 0x80
    test rax, rax
    jnz user_fail
    mov eax, 1
    mov edx, 1
    int 0x80
    test rax, rax
    jnz user_fail
    mov eax, 3
    int 0x80
    test rax, rax
    jnz user_fail
    mov eax, 1
    int 0x80
    cmp rax, -1
    jne user_fail
    mov eax, 9                  # Reject empty objects and truncated fill values.
    xor edi, edi
    xor esi, esi
    int 0x80
    cmp rax, -1
    jne user_fail
    mov eax, 9
    mov edi, 4097
    mov esi, 256
    int 0x80
    cmp rax, -1
    jne user_fail
    mov eax, 9                  # Create an object in this real user task.
    mov esi, 0x33
    int 0x80
    cmp rax, -1
    je user_fail
    mov r13, rax
    mov rdi, r13
    mov eax, 1
    mov esi, 0x40000000
    mov edx, 1
    int 0x80
    test rax, rax
    jnz user_fail
    cmp byte ptr [0x40000000], 0x33
    jne user_fail
    cmp byte ptr [0x40001000], 0x33
    jne user_fail
    cmp byte ptr [0x40001001], 0
    jne user_fail
    mov eax, 8
    xor edi, edi
    int 0x80
user_fail:
    mov eax, 8
    mov edi, 1
    int 0x80
    ud2
.global m4_user_end
m4_user_end:
