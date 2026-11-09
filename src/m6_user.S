.section .rodata.m6_user,"a",@progbits
.global m6_user_start
.global m6_user_end
m6_user_start:
    // r12=task, r13=byte length, r14=number of pages to mutate in task 0.
    call 7f
    xor rdi, rdi
    mov rax, 2
    int 0x80
    test r12, r12
    jnz 2f
    mov rcx, r14
    test rcx, rcx
    jz 2f
    mov rbx, 0x40000000
1:  xor byte ptr [rbx], 0xff
    add rbx, 4096
    dec rcx
    jnz 1b
2:  call 7f
    mov rdi, 1
    mov rax, 2
    int 0x80
    xor rdi, rdi
    mov rax, 8
    int 0x80
    ud2
7:  mov rbx, 0x40000000
    mov rcx, r13
    xor rsi, rsi
6:  movzx rax, byte ptr [rbx]
    add rsi, rax
    inc rbx
    dec rcx
    jnz 6b
    ret
m6_user_end:
