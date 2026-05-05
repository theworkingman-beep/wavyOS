// aarch64 context switch
// void switch_context(old_rsp_ptr: *mut usize, new_rsp: usize)
// Saves old SP to *old_rsp_ptr, loads new SP from new_rsp
// Preserves callee-saved registers: x19-x28, x29(x29), x30(lr)

.global switch_context
switch_context:
    cbz x0, 1f
    stp x29, x30, [sp, #-16]!
    stp x27, x28, [sp, #-16]!
    stp x25, x26, [sp, #-16]!
    stp x23, x24, [sp, #-16]!
    stp x21, x22, [sp, #-16]!
    stp x19, x20, [sp, #-16]!
    mov x2, sp
    str x2, [x0]
1:
    mov sp, x1
    ldp x19, x20, [sp], #16
    ldp x21, x22, [sp], #16
    ldp x23, x24, [sp], #16
    ldp x25, x26, [sp], #16
    ldp x27, x28, [sp], #16
    ldp x29, x30, [sp], #16
    ret
