/* Dedicated normal-execution Hyp capture, R52 TRM Tables 10-69/80.
 * Startup must establish static interrupt state independently. This hook does
 * not enable/acknowledge/deactivate interrupts or alter CPU mode. Assign each
 * core its own slot; do not overwrite another core's snapshot. LR and LRC are
 * separate MRC samples, never an atomic 64-bit or cross-core snapshot. */
#include <stdint.h>
struct debugtui_gic_snapshot { uint32_t ready, error, cpsr, midr, id_pfr1, hcr, hstr, raw[29]; };
volatile struct debugtui_gic_snapshot debugtui_gic_reference[2];
#define READ(o,n,m,p) ({ uint32_t result; __asm__ volatile("mrc p15," #o ",%0,c" #n ",c" #m "," #p : "=r"(result)); result; })
__attribute__((noinline)) void debugtui_gic_fixture_stop(void) { __asm__ volatile("" ::: "memory"); }
void debugtui_gic_capture(volatile struct debugtui_gic_snapshot *snapshot)
{
    snapshot->ready=0; snapshot->error=1;
    __asm__ volatile("mrs %0,cpsr" : "=r"(snapshot->cpsr));
    if ((snapshot->cpsr & 31) != 26) return;
    snapshot->midr=READ(0,0,0,0);snapshot->id_pfr1=READ(0,0,1,1);snapshot->error=2;
    if ((snapshot->midr & 0xff0ffff0) != 0x410fd130 || (snapshot->id_pfr1 >> 28) != 1) return;
    snapshot->raw[2]=READ(4,12,9,5);snapshot->error=3;
    if (snapshot->raw[2] != 15) return;
    snapshot->raw[1]=READ(0,12,12,5);
    if (snapshot->raw[1] != 7) return;
    snapshot->raw[0]=READ(0,12,12,4);
    if ((snapshot->raw[0] & ~UINT32_C(3)) != UINT32_C(0x400)) return;
    snapshot->raw[13]=READ(4,12,11,1);
    if (snapshot->raw[13] != UINT32_C(0x90180003)) return;
    snapshot->hcr=READ(4,1,1,0);snapshot->hstr=READ(4,1,1,3);
    snapshot->raw[3]=READ(0,4,6,0); /* ICC_PMR */
    snapshot->raw[4]=READ(0,12,11,3); /* ICC_RPR */
    snapshot->raw[5]=READ(0,12,8,3); /* ICC_BPR0 */
    snapshot->raw[6]=READ(0,12,12,3); /* ICC_BPR1 */
    snapshot->raw[7]=READ(0,12,12,6); /* ICC_IGRPEN0 */
    snapshot->raw[8]=READ(0,12,12,7); /* ICC_IGRPEN1 */
    snapshot->raw[9]=READ(0,12,8,2); /* ICC_HPPIR0 */
    snapshot->raw[10]=READ(0,12,12,2); /* ICC_HPPIR1 */
    snapshot->raw[11]=READ(0,12,8,4); /* ICC_AP0R0 */
    snapshot->raw[12]=READ(0,12,9,0); /* ICC_AP1R0 */
    snapshot->raw[14]=READ(4,12,11,0); /* ICH_HCR */
    snapshot->raw[15]=READ(4,12,11,2); /* ICH_MISR */
    snapshot->raw[16]=READ(4,12,11,3); /* ICH_EISR */
    snapshot->raw[17]=READ(4,12,11,5); /* ICH_ELRSR */
    snapshot->raw[18]=READ(4,12,11,7); /* ICH_VMCR */
    snapshot->raw[19]=READ(4,12,8,0); /* ICH_AP0R0 */
    snapshot->raw[20]=READ(4,12,9,0); /* ICH_AP1R0 */
    snapshot->raw[21]=READ(4,12,12,0); /* ICH_LR0 */
    snapshot->raw[22]=READ(4,12,12,1); /* ICH_LR1 */
    snapshot->raw[23]=READ(4,12,12,2); /* ICH_LR2 */
    snapshot->raw[24]=READ(4,12,12,3); /* ICH_LR3 */
    snapshot->raw[25]=READ(4,12,14,0); /* ICH_LRC0 */
    snapshot->raw[26]=READ(4,12,14,1); /* ICH_LRC1 */
    snapshot->raw[27]=READ(4,12,14,2); /* ICH_LRC2 */
    snapshot->raw[28]=READ(4,12,14,3); /* ICH_LRC3 */
    snapshot->error=4;
    if (snapshot->raw[0] != READ(0,12,12,4) || snapshot->raw[13] != READ(4,12,11,1) ||
        snapshot->hcr != READ(4,1,1,0) || snapshot->hstr != READ(4,1,1,3)) return;
    snapshot->error=0;snapshot->ready=1;debugtui_gic_fixture_stop();
}
