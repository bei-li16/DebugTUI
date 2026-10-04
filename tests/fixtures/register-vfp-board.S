/* SPDX-License-Identifier: Apache-2.0
 * Independent REG-H03 firmware hook. Startup establishes mode, FPU data and
 * permissions; this hook only reads FP/CP15 state, never enables or writes it.
 * Stop in the ready loop, using core-private references and ready flags.
 * reference[0..6] = HCPTR,FPSID,MVFR0,MVFR1,MVFR2,FPEXC,FPSCR;
 * reference[8+2*n..9+2*n] = raw Dn, low word first, only when legal and EN=1.
 */
#ifndef DEBUGTUI_VFP_REFERENCE
#define DEBUGTUI_VFP_REFERENCE debugtui_vfp_reference
#endif
#ifndef DEBUGTUI_VFP_READY
#define DEBUGTUI_VFP_READY debugtui_vfp_reference_ready
#endif
#ifndef DEBUGTUI_VFP_HOOK
#define DEBUGTUI_VFP_HOOK debugtui_vfp_board_fixture
#endif
.syntax unified
.thumb
.fpu neon-fp-armv8
.text
.align 2
.global DEBUGTUI_VFP_HOOK
.type DEBUGTUI_VFP_HOOK,%function
.thumb_func
DEBUGTUI_VFP_HOOK:
    mrs r0,cpsr
    and r0,r0,#31
    cmp r0,#0x1a
    bne .Lwrong_mode
    ldr r2,=DEBUGTUI_VFP_REFERENCE
    mrc p15,4,r0,c1,c1,2
    str r0,[r2,#0]
    tst r0,#(1<<10)
    bne .Lready_flag
.macro CONTROL index, name
    vmrs r0,\name
    str r0,[r2,#(4*\index)]
.endm
    CONTROL 1,fpsid
    CONTROL 2,mvfr0
    CONTROL 3,mvfr1
    CONTROL 4,mvfr2
    CONTROL 5,fpexc
    tst r0,#(1<<30)
    beq .Lready_flag
    CONTROL 6,fpscr
.macro DATA index
    vmov r0,r1,d\index
    str r0,[r2,#(32+8*\index)]
    str r1,[r2,#(36+8*\index)]
.endm
.irp n,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15
    DATA \n
.endr
    ldr r0,[r2,#8]
    and r0,r0,#15
    cmp r0,#2
    bne .Lready_flag
.irp n,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    DATA \n
.endr
.Lready_flag:
    ldr r2,=DEBUGTUI_VFP_READY
    mov r0,#1
    str r0,[r2]
.Lready:
    b .Lready
.Lwrong_mode:
    b .Lwrong_mode
.size DEBUGTUI_VFP_HOOK,.-DEBUGTUI_VFP_HOOK
.comm DEBUGTUI_VFP_REFERENCE,288,4
.comm DEBUGTUI_VFP_READY,4,4
.section .note.GNU-stack,"",%progbits
