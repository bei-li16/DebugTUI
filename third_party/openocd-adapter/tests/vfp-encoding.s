.syntax unified
.cpu cortex-r52
.fpu neon-fp-armv8
.thumb
.text
.global vfp_encodings
vfp_encodings:
    vmrs r0,fpsid
    vmrs r0,fpscr
    vmrs r0,mvfr0
    vmrs r0,mvfr1
    vmrs r0,mvfr2
    vmrs r0,fpexc
    vmov r0,r1,d0
    vmov r0,r1,d15
    vmov r0,r1,d16
    vmov r0,r1,d31
    vmov d0,r0,r1
    vmov d15,r0,r1
    vmov d16,r0,r1
    vmov d31,r0,r1
    mrc p15,4,r0,c1,c1,2
