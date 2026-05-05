# x86_64 context switch
# void switch_context(old_rsp_ptr: *mut usize, new_rsp: usize)
# Saves old RSP to *old_rsp_ptr, loads new RSP from new_rsp
# Preserves callee-saved registers: rbx, rbp, r12-r15

.global switch_context
switch_context:
    test rdi, rdi
    jz 1f
    push r15
    push r14
    push r13
    push r12
    push rbp
    push rbx
    mov [rdi], rsp
1:
    mov rsp, rsi
    pop rbx
    pop rbp
    pop r12
    pop r13
    pop r14
    pop r15
    ret
