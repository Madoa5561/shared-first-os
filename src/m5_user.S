.section .rodata.m5_user,"a",@progbits
.global m5_user_start
.global m5_user_end
m5_user_start:
    mov rax, 1
    mov rdi, r13
    mov rsi, 0x40000000
    mov rdx, 1
    test r12, r12
    jnz 1f
    mov rdx, 64
1:  int 0x80
    test rax, rax
    jnz 9f
    call 7f
    xor rdi, rdi
    mov rax, 2
    int 0x80
    test r12, r12
    jnz 2f
    mov rbx, 0x40000000
    xor byte ptr [rbx], 0xff
2:  call 7f
    mov rdi, 1
    mov rax, 2
    int 0x80
    // Kernel revokes the writer's object while suspended here.
    call 7f
    mov rdi, 2
    mov rax, 2
    int 0x80
    xor rdi, rdi
    jmp 8f
7:  mov rbx, 0x40000000
    mov rcx, 16384
    xor rsi, rsi
6:  movzx rax, byte ptr [rbx]
    add rsi, rax
    inc rbx
    dec rcx
    jnz 6b
    ret
9:  mov rdi, 1
8:  mov rax, 8
    int 0x80
    ud2
m5_user_end:
