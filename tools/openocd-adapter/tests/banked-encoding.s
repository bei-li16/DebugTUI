.syntax unified
.cpu cortex-r52
.thumb
.text
.global banked_encodings
banked_encodings:
    mrc p15, 3, r0, c4, c5, 0
    mrs r0, cpsr
    mrs r0, spsr
    mov.w r0, r8
    mov.w r0, sp
    mov.w r0, lr
    mrs r0, r8_usr
    mrs r0, sp_usr
    mrs r0, lr_usr
    mrs r0, r8_fiq
    mrs r0, r12_fiq
    mrs r0, sp_fiq
    mrs r0, lr_fiq
    mrs r0, spsr_fiq
    mrs r0, lr_irq
    mrs r0, sp_irq
    mrs r0, spsr_irq
    mrs r0, lr_svc
    mrs r0, sp_svc
    mrs r0, spsr_svc
    mrs r0, lr_abt
    mrs r0, sp_abt
    mrs r0, spsr_abt
    mrs r0, lr_und
    mrs r0, sp_und
    mrs r0, spsr_und
    mrs r0, elr_hyp
    mrs r0, sp_hyp
    mrs r0, spsr_hyp
