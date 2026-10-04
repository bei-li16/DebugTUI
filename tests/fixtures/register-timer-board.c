/* Dedicated read-only REG-H05 hook; GNU Arm compiler, actual R52 Generic Timer.
 * Startup must establish permissions, a known nonzero-high-word CVAL and any
 * debug freeze behavior before this hook. No timer/control write occurs here.
 * Build separately for each core or store references in per-core arrays. */
#include <stdint.h>
volatile uint64_t debugtui_timer_reference_cntpct;
volatile uint64_t debugtui_timer_reference_cntvct;
volatile uint64_t debugtui_timer_reference_cntp_cval;
volatile uint32_t debugtui_timer_ready;
#define READ_TIMER(op) ({ uint32_t lo,hi; __asm__ volatile("mrrc p15, " #op ", %0, %1, c14" : "=r"(lo), "=r"(hi)); ((uint64_t)hi<<32)|lo; })
void debugtui_timer_board_fixture(void)
{
    debugtui_timer_reference_cntpct=READ_TIMER(0);
    debugtui_timer_reference_cntvct=READ_TIMER(1);
    debugtui_timer_reference_cntp_cval=READ_TIMER(2);
    debugtui_timer_ready=1;
    while (debugtui_timer_ready) __asm__ volatile("" ::: "memory");
}
