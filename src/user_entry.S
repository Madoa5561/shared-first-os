.section .text
.global m4_enter
m4_enter:
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15
    mov [rip + m4_kernel_rsp], rsp
    mov [rip + m4_context], rdi
    mov [rip + m4_kernel_root], rdx
    mov cr3, rsi
    mov ax, 0x1b
    mov ds, ax
    mov es, ax
    mov rsp, rdi
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    add rsp, 16
    iretq

.macro VECTOR number, error=0
.global m4_vector_\number
m4_vector_\number:
.if \error == 0
    push 0
.endif
    push \number
    jmp m4_trap
.endm
VECTOR 0
VECTOR 1
VECTOR 2
VECTOR 3
VECTOR 4
VECTOR 5
VECTOR 6
VECTOR 7
VECTOR 8, 1
VECTOR 9
VECTOR 10, 1
VECTOR 11, 1
VECTOR 12, 1
VECTOR 13, 1
VECTOR 14, 1
VECTOR 15
VECTOR 16
VECTOR 17, 1
VECTOR 18
VECTOR 19
VECTOR 20
VECTOR 21, 1
VECTOR 22
VECTOR 23
VECTOR 24
VECTOR 25
VECTOR 26
VECTOR 27
VECTOR 28
VECTOR 29, 1
VECTOR 30, 1
VECTOR 31
VECTOR 128
VECTOR 255

m4_trap:
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    cld
    test byte ptr [rsp + 144], 3
    jz m4_kernel_fault
    mov rsi, rsp
    mov rdi, [rip + m4_context]
    mov ecx, 22
    rep movsq
    mov rax, cr2
    mov [rip + m4_fault_address], rax
    mov rax, cr3
    mov [rip + m4_observed_root], rax
    mov rax, [rip + m4_kernel_root]
    mov cr3, rax
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov rsp, [rip + m4_kernel_rsp]
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret

m4_kernel_fault:
    mov rdi, [rsp + 120]
    mov rsi, [rsp + 128]
    mov rdx, [rsp + 136]
    and rsp, -16
    call m4_fatal
    ud2

.section .rodata
.global m4_vectors
m4_vectors:
    .quad m4_vector_0, m4_vector_1, m4_vector_2, m4_vector_3
    .quad m4_vector_4, m4_vector_5, m4_vector_6, m4_vector_7
    .quad m4_vector_8, m4_vector_9, m4_vector_10, m4_vector_11
    .quad m4_vector_12, m4_vector_13, m4_vector_14, m4_vector_15
    .quad m4_vector_16, m4_vector_17, m4_vector_18, m4_vector_19
    .quad m4_vector_20, m4_vector_21, m4_vector_22, m4_vector_23
    .quad m4_vector_24, m4_vector_25, m4_vector_26, m4_vector_27
    .quad m4_vector_28, m4_vector_29, m4_vector_30, m4_vector_31

.section .bss
.balign 8
m4_kernel_rsp: .quad 0
m4_kernel_root: .quad 0
m4_context: .quad 0
.global m4_fault_address
m4_fault_address: .quad 0
.global m4_observed_root
m4_observed_root: .quad 0
