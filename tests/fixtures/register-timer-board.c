/* Dedicated read-only REG-H05 hook; GNU Arm compiler, actual R52 Generic Timer.
 * Startup must establish permissions, a known nonzero-high-word CVAL and any
 * debug freeze behavior before this hook. No timer/control write occurs here.
 * Build separately for each core or store references in per-core arrays. */
#include <stdint.h>
volatile uint64_t debugtui_timer_reference_cntpct;
volatile uint64_t debugtui_timer_reference_cntvct;
volatile uint64_t debugtui_timer_reference_cntp_cval;
volatile uint64_t debugtui_timer_reference_cntv_cval;
volatile uint64_t debugtui_timer_reference_cntvoff;
volatile uint64_t debugtui_timer_reference_cnthp_cval;
/* CNTFRQ, CNTKCTL, CNTP_TVAL/CTL, CNTV_TVAL/CTL, CNTHCTL, CNTHP_TVAL/CTL.
 * TVAL is dynamic, and TVAL/ISTATUS may be UNKNOWN when disabled. */
volatile uint32_t debugtui_timer_reference_controls[9];
volatile uint32_t debugtui_timer_ready;
#define READ_TIMER(op) ({ uint32_t lo,hi; __asm__ volatile("mrrc p15, " #op ", %0, %1, c14" : "=r"(lo), "=r"(hi)); ((uint64_t)hi<<32)|lo; })
#define READ_CONTROL(op,crm,op2) ({ uint32_t value; __asm__ volatile("mrc p15, " #op ", %0, c14, c" #crm ", " #op2 : "=r"(value)); value; })
void debugtui_timer_board_fixture(void)
{
    uint32_t cpsr;
    debugtui_timer_ready=0;
    __asm__ volatile("mrs %0, cpsr" : "=r"(cpsr));
    if ((cpsr & 31) != 26) return;
    debugtui_timer_reference_controls[0]=READ_CONTROL(0,0,0);
    debugtui_timer_reference_controls[1]=READ_CONTROL(0,1,0);
    debugtui_timer_reference_controls[2]=READ_CONTROL(0,2,0);
    debugtui_timer_reference_controls[3]=READ_CONTROL(0,2,1);
    debugtui_timer_reference_controls[4]=READ_CONTROL(0,3,0);
    debugtui_timer_reference_controls[5]=READ_CONTROL(0,3,1);
    debugtui_timer_reference_controls[6]=READ_CONTROL(4,1,0);
    debugtui_timer_reference_controls[7]=READ_CONTROL(4,2,0);
    debugtui_timer_reference_controls[8]=READ_CONTROL(4,2,1);
    debugtui_timer_reference_cntpct=READ_TIMER(0);
    debugtui_timer_reference_cntvct=READ_TIMER(1);
    debugtui_timer_reference_cntp_cval=READ_TIMER(2);
    debugtui_timer_reference_cntv_cval=READ_TIMER(3);
    debugtui_timer_reference_cntvoff=READ_TIMER(4);
    debugtui_timer_reference_cnthp_cval=READ_TIMER(6);
    /* Publish only after the reference stores. This is not a cache clean;
     * the board case must independently prove debugger/RAM visibility. */
    __asm__ volatile("dmb sy" ::: "memory");
    debugtui_timer_ready=1;
    while (debugtui_timer_ready) __asm__ volatile("" ::: "memory");
}
