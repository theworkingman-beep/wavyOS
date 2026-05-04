// aarch64 exception vector table for QEMU virt machine
// Loaded into VBAR_EL1 at boot

    .pushsection .text.vector_table_el1, "ax"
    .globl __vector_table_el1
    .balign 0x800
__vector_table_el1:
    // Current EL, SP0
    .balign 0x80
    b handle_sync_el1_sp0
    .balign 0x80
    b handle_irq_el1_sp0
    .balign 0x80
    b handle_fiq_el1_sp0
    .balign 0x80
    b handle_serror_el1_sp0

    // Current EL, SPx
    .balign 0x80
    b handle_sync_el1_spx
    .balign 0x80
    b handle_irq_el1_spx
    .balign 0x80
    b handle_fiq_el1_spx
    .balign 0x80
    b handle_serror_el1_spx

    // Lower EL, AArch64
    .balign 0x80
    b handle_sync_lower_aarch64
    .balign 0x80
    b handle_irq_lower_aarch64
    .balign 0x80
    b handle_fiq_lower_aarch64
    .balign 0x80
    b handle_serror_lower_aarch64

    // Lower EL, AArch32
    .balign 0x80
    b handle_sync_lower_aarch32
    .balign 0x80
    b handle_irq_lower_aarch32
    .balign 0x80
    b handle_fiq_lower_aarch32
    .balign 0x80
    b handle_serror_lower_aarch32

    // SP0 handlers (should not happen)
handle_sync_el1_sp0:
handle_irq_el1_sp0:
handle_fiq_el1_sp0:
handle_serror_el1_sp0:
    b .

    // SPx handlers
handle_sync_el1_spx:
    stp x0, x1, [sp, #-16]!
    stp x2, x3, [sp, #-16]!
    stp x4, x5, [sp, #-16]!
    stp x6, x7, [sp, #-16]!
    stp x8, x9, [sp, #-16]!
    stp x10, x11, [sp, #-16]!
    stp x12, x13, [sp, #-16]!
    stp x14, x15, [sp, #-16]!
    stp x16, x17, [sp, #-16]!
    stp x18, x19, [sp, #-16]!
    stp x20, x21, [sp, #-16]!
    stp x22, x23, [sp, #-16]!
    stp x24, x25, [sp, #-16]!
    stp x26, x27, [sp, #-16]!
    stp x28, x29, [sp, #-16]!
    stp x30, xzr, [sp, #-16]!
    bl handle_sync_el1
    ldp x30, xzr, [sp], #16
    ldp x28, x29, [sp], #16
    ldp x26, x27, [sp], #16
    ldp x24, x25, [sp], #16
    ldp x22, x23, [sp], #16
    ldp x20, x21, [sp], #16
    ldp x18, x19, [sp], #16
    ldp x16, x17, [sp], #16
    ldp x14, x15, [sp], #16
    ldp x12, x13, [sp], #16
    ldp x10, x11, [sp], #16
    ldp x8, x9, [sp], #16
    ldp x6, x7, [sp], #16
    ldp x4, x5, [sp], #16
    ldp x2, x3, [sp], #16
    ldp x0, x1, [sp], #16
    eret

handle_irq_el1_spx:
    stp x0, x1, [sp, #-16]!
    stp x2, x3, [sp, #-16]!
    stp x4, x5, [sp, #-16]!
    stp x6, x7, [sp, #-16]!
    stp x8, x9, [sp, #-16]!
    stp x10, x11, [sp, #-16]!
    stp x12, x13, [sp, #-16]!
    stp x14, x15, [sp, #-16]!
    stp x16, x17, [sp, #-16]!
    stp x18, x19, [sp, #-16]!
    stp x20, x21, [sp, #-16]!
    stp x22, x23, [sp, #-16]!
    stp x24, x25, [sp, #-16]!
    stp x26, x27, [sp, #-16]!
    stp x28, x29, [sp, #-16]!
    stp x30, xzr, [sp, #-16]!
    bl handle_irq_el1
    ldp x30, xzr, [sp], #16
    ldp x28, x29, [sp], #16
    ldp x26, x27, [sp], #16
    ldp x24, x25, [sp], #16
    ldp x22, x23, [sp], #16
    ldp x20, x21, [sp], #16
    ldp x18, x19, [sp], #16
    ldp x16, x17, [sp], #16
    ldp x14, x15, [sp], #16
    ldp x12, x13, [sp], #16
    ldp x10, x11, [sp], #16
    ldp x8, x9, [sp], #16
    ldp x6, x7, [sp], #16
    ldp x4, x5, [sp], #16
    ldp x2, x3, [sp], #16
    ldp x0, x1, [sp], #16
    eret

handle_fiq_el1_spx:
    b handle_fiq_el1

handle_serror_el1_spx:
    b handle_serror_el1

    // Lower EL, AArch64 handlers
handle_sync_lower_aarch64:
handle_sync_lower_aarch32:
    stp x0, x1, [sp, #-16]!
    stp x2, x3, [sp, #-16]!
    stp x4, x5, [sp, #-16]!
    stp x6, x7, [sp, #-16]!
    stp x8, x9, [sp, #-16]!
    stp x10, x11, [sp, #-16]!
    stp x12, x13, [sp, #-16]!
    stp x14, x15, [sp, #-16]!
    stp x16, x17, [sp, #-16]!
    stp x18, x19, [sp, #-16]!
    stp x20, x21, [sp, #-16]!
    stp x22, x23, [sp, #-16]!
    stp x24, x25, [sp, #-16]!
    stp x26, x27, [sp, #-16]!
    stp x28, x29, [sp, #-16]!
    stp x30, xzr, [sp, #-16]!
    bl handle_sync_lower_el
    ldp x30, xzr, [sp], #16
    ldp x28, x29, [sp], #16
    ldp x26, x27, [sp], #16
    ldp x24, x25, [sp], #16
    ldp x22, x23, [sp], #16
    ldp x20, x21, [sp], #16
    ldp x18, x19, [sp], #16
    ldp x16, x17, [sp], #16
    ldp x14, x15, [sp], #16
    ldp x12, x13, [sp], #16
    ldp x10, x11, [sp], #16
    ldp x8, x9, [sp], #16
    ldp x6, x7, [sp], #16
    ldp x4, x5, [sp], #16
    ldp x2, x3, [sp], #16
    ldp x0, x1, [sp], #16
    eret

handle_irq_lower_aarch64:
handle_irq_lower_aarch32:
    b handle_irq_el1_spx

handle_fiq_lower_aarch64:
handle_fiq_lower_aarch32:
    b handle_fiq_el1

handle_serror_lower_aarch64:
handle_serror_lower_aarch32:
    b handle_serror_el1
