/* SPDX-License-Identifier: Apache-2.0
 * Independent GNU Arm REG-H02 sample hook; contains no CPS/MSR/mode switch.
 * Call from the declared mode, then stop in the ready loop. Do not break at
 * function entry. Use core-private reference/ready storage or distinct symbols
 * for each core (DEBUGTUI_BANKED_REFERENCE and DEBUGTUI_BANKED_READY).
 * Startup establishes bank contents and permissions. The hook never returns.
 */
#ifndef DEBUGTUI_BANKED_MODE
#define DEBUGTUI_BANKED_MODE 0x1a
#endif
#ifndef DEBUGTUI_BANKED_REFERENCE
#define DEBUGTUI_BANKED_REFERENCE debugtui_banked_reference
#endif
#ifndef DEBUGTUI_BANKED_READY
#define DEBUGTUI_BANKED_READY debugtui_banked_ready
#endif
.syntax unified
.thumb
.text
.align 2
.global debugtui_banked_board_fixture
.type debugtui_banked_board_fixture,%function
.thumb_func
debugtui_banked_board_fixture:
#if DEBUGTUI_BANKED_MODE != 0x10
    /* User MRS CPSR does not reliably expose M. The external GDB/DSPSR
     * guard validates User mode; do not inject a privileged software probe. */
    mrs r0,cpsr
    and r0,r0,#31
    cmp r0,#DEBUGTUI_BANKED_MODE
    bne .Lwrong_mode
#endif
    ldr r1,=DEBUGTUI_BANKED_REFERENCE
.macro SAMPLE index, bank, bankmode, current
#if DEBUGTUI_BANKED_MODE == 0x10
    mov r0,#0
#else
    .if DEBUGTUI_BANKED_MODE == \bankmode
        \current
    .else
        mrs r0,\bank
    .endif
#endif
    str r0,[r1,#(4*\index)]
.endm
    SAMPLE 0,sp_irq,0x12,"mov r0,sp"
    SAMPLE 1,lr_irq,0x12,"mov r0,lr"
    SAMPLE 2,spsr_irq,0x12,"mrs r0,spsr"
    SAMPLE 3,r8_fiq,0x11,"mov r0,r8"
    SAMPLE 4,r9_fiq,0x11,"mov r0,r9"
    SAMPLE 5,r10_fiq,0x11,"mov r0,r10"
    SAMPLE 6,r11_fiq,0x11,"mov r0,r11"
    SAMPLE 7,r12_fiq,0x11,"mov r0,r12"
    SAMPLE 8,sp_fiq,0x11,"mov r0,sp"
    SAMPLE 9,lr_fiq,0x11,"mov r0,lr"
    SAMPLE 10,spsr_fiq,0x11,"mrs r0,spsr"
    SAMPLE 11,sp_und,0x1b,"mov r0,sp"
    SAMPLE 12,lr_und,0x1b,"mov r0,lr"
    SAMPLE 13,spsr_und,0x1b,"mrs r0,spsr"
    SAMPLE 14,sp_abt,0x17,"mov r0,sp"
    SAMPLE 15,lr_abt,0x17,"mov r0,lr"
    SAMPLE 16,spsr_abt,0x17,"mrs r0,spsr"
    SAMPLE 17,sp_svc,0x13,"mov r0,sp"
    SAMPLE 18,lr_svc,0x13,"mov r0,lr"
    SAMPLE 19,spsr_svc,0x13,"mrs r0,spsr"
#if DEBUGTUI_BANKED_MODE == 0x1a
    mov r0,sp
    str r0,[r1,#80]
    mrs r0,elr_hyp
    str r0,[r1,#84]
    mrs r0,spsr
    str r0,[r1,#88]
#else
    mov r0,#0
    str r0,[r1,#80]
    str r0,[r1,#84]
    str r0,[r1,#88]
#endif
    /* Explicit User-bank reference slots for the optional thirty-name catalogue.
     * This hook runs in normal firmware, where direct CPSR is defined. */
#if DEBUGTUI_BANKED_MODE == 0x11
    mrs r0,r8_usr
    str r0,[r1,#92]
    mrs r0,r9_usr
    str r0,[r1,#96]
    mrs r0,r10_usr
    str r0,[r1,#100]
    mrs r0,r11_usr
    str r0,[r1,#104]
    mrs r0,r12_usr
    str r0,[r1,#108]
#else
    mov r0,r8
    str r0,[r1,#92]
    mov r0,r9
    str r0,[r1,#96]
    mov r0,r10
    str r0,[r1,#100]
    mov r0,r11
    str r0,[r1,#104]
    mov r0,r12
    str r0,[r1,#108]
#endif
#if DEBUGTUI_BANKED_MODE == 0x10 || DEBUGTUI_BANKED_MODE == 0x1f
    mov r0,sp
#else
    mrs r0,sp_usr
#endif
    str r0,[r1,#112]
#if DEBUGTUI_BANKED_MODE == 0x10 || DEBUGTUI_BANKED_MODE == 0x1f || DEBUGTUI_BANKED_MODE == 0x1a
    mov r0,lr
#else
    mrs r0,lr_usr
#endif
    str r0,[r1,#116]
    ldr r1,=DEBUGTUI_BANKED_READY
    mov r0,#1
    str r0,[r1]
.Lready:
    b .Lready
.Lwrong_mode:
    b .Lwrong_mode
.size debugtui_banked_board_fixture,.-debugtui_banked_board_fixture
.comm DEBUGTUI_BANKED_REFERENCE,120,4
.comm DEBUGTUI_BANKED_READY,4,4
.section .note.GNU-stack,"",%progbits
