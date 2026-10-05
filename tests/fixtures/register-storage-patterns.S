/* SPDX-License-Identifier: Apache-2.0
 * Dedicated assembly test-startup seed for REG-305, not a general C/AAPCS
 * function: it intentionally replaces D8-D15 as well as volatile FP registers.
 * Startup must establish FP permissions and EN before calling this helper.
 * This helper writes only test FP data, never CPU mode or FP/system controls.
 * Invoke the separate register-vfp-board.S sampling hook after initialization.
 */
.syntax unified
.thumb
.fpu neon-fp-armv8
.text
.align 2
.global debugtui_storage_pattern_seed
.type debugtui_storage_pattern_seed,%function
.thumb_func
debugtui_storage_pattern_seed:
    ldr r2,=debugtui_storage_pattern_words
.macro SEED n
    ldr r0,[r2,#(8*\n)]
    ldr r1,[r2,#(4+8*\n)]
    vmov d\n,r0,r1
.endm
.irp n,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15
    SEED \n
.endr
#if DEBUGTUI_STORAGE_D32
.irp n,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    SEED \n
.endr
#endif
    bx lr
.size debugtui_storage_pattern_seed,.-debugtui_storage_pattern_seed
.section .rodata
.align 2
.global debugtui_storage_pattern_words
debugtui_storage_pattern_words:
    .word 0x00000000,0x80000000,0x7f800000,0xff800000
    .word 0x7fc00042,0x7f800001,0xffc00042,0xff800001
    .word 0x00000001,0x80000001,0x7f7fffff,0xff7fffff
    .word 0x3f800000,0xbf800000,0x00800000,0x80800000
    .word 0x00000000,0x00000000,0x00000000,0x80000000
    .word 0x00000000,0x7ff00000,0x00000000,0xfff00000
    .word 0x00000042,0x7ff80000,0x00000042,0x7ff00000
    .word 0x00000042,0xfff80000,0x00000042,0xfff00000
    .word 0x00000001,0x00000000,0x00000001,0x80000000
    .word 0xffffffff,0x7fefffff,0xffffffff,0xffefffff
    .word 0x00000000,0x3ff00000,0x00000000,0xbff00000
    .word 0x00000000,0x00100000,0x00000000,0x80100000
    .word 0x89abcdef,0x01234567,0x76543210,0xfedcba98
    .word 0x55667788,0x11223344,0x44332211,0x88776655
    .word 0x80000000,0x01234567,0xdeadbeef,0x7ff80000
    .word 0x5a5a5a5a,0xa5a5a5a5,0xfedcba98,0x12345678
 .size debugtui_storage_pattern_words,.-debugtui_storage_pattern_words
.section .note.GNU-stack,"",%progbits
