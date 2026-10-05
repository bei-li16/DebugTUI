/* Dedicated normal-execution Hyp PMU capture; this hook never changes PMU
 * controls, CPU mode, PMSELR or counter values. Integrate in reviewed firmware.
 * Startup must leave PMCR.E=0, HDCR.HPME=0, SEL<4 and a nonzero cycle high word.
 * Each physical core must receive a distinct snapshot; never share one slot. */
#include <stdint.h>
struct debugtui_pmu_snapshot {
    uint32_t ready, error, cpsr, midr, id_dfr0, hdcr, raw[22];
    uint64_t cycle;
};
volatile struct debugtui_pmu_snapshot debugtui_pmu_reference[2];
#define READ32(n,m,o) ({ uint32_t result; __asm__ volatile("mrc p15,0,%0,c" #n ",c" #m "," #o : "=r"(result)); result; })
#define READH32(n,m,o) ({ uint32_t result; __asm__ volatile("mrc p15,4,%0,c" #n ",c" #m "," #o : "=r"(result)); result; })
__attribute__((noinline)) void debugtui_pmu_fixture_stop(void)
{
    __asm__ volatile("" ::: "memory");
}
void debugtui_pmu_capture(volatile struct debugtui_pmu_snapshot *snapshot)
{
    snapshot->ready = 0;
    __asm__ volatile("mrs %0,cpsr" : "=r"(snapshot->cpsr));
    snapshot->error = 1;
    if ((snapshot->cpsr & 31) != 26) return;
    snapshot->midr = READ32(0,0,0);
    snapshot->id_dfr0 = READ32(0,1,2);
    snapshot->error = 2;
    if ((snapshot->midr & 0xff0ffff0) != 0x410fd130 ||
        ((snapshot->id_dfr0 >> 24) & 15) != 3) return;
    snapshot->raw[0] = READ32(9,12,0);
    snapshot->hdcr = READH32(1,1,1);
    snapshot->raw[4] = READ32(9,12,5);
    snapshot->error = 3;
    if ((snapshot->raw[0] & 0xffffff80) != 0x41132000 ||
        (snapshot->raw[0] & 1) || (snapshot->hdcr & 128) || snapshot->raw[4] >= 4) return;
    snapshot->raw[1] = READ32(9,12,1); snapshot->raw[2] = READ32(9,12,2);
    snapshot->raw[3] = READ32(9,12,3);
    snapshot->raw[5] = READ32(9,12,6); snapshot->raw[6] = READ32(9,12,7);
    snapshot->raw[7] = READ32(9,13,1); snapshot->raw[8] = READ32(9,13,2);
    snapshot->raw[9] = READ32(9,14,0); snapshot->raw[10] = READ32(9,14,1);
    snapshot->raw[11] = READ32(9,14,2); snapshot->raw[12] = READ32(9,14,3);
    snapshot->raw[13] = READ32(14,15,7);
    snapshot->raw[14] = READ32(14,8,0); snapshot->raw[15] = READ32(14,8,1);
    snapshot->raw[16] = READ32(14,8,2); snapshot->raw[17] = READ32(14,8,3);
    snapshot->raw[18] = READ32(14,12,0); snapshot->raw[19] = READ32(14,12,1);
    snapshot->raw[20] = READ32(14,12,2); snapshot->raw[21] = READ32(14,12,3);
    uint32_t low, high;
    __asm__ volatile("mrrc p15,0,%0,%1,c9" : "=r"(low), "=r"(high));
    snapshot->cycle = ((uint64_t)high << 32) | low;
    snapshot->error = 4;
    if (snapshot->raw[0] != READ32(9,12,0) || snapshot->raw[4] != READ32(9,12,5) ||
        snapshot->hdcr != READH32(1,1,1)) return;
    snapshot->error = 5;
    if (high == 0) return;
    snapshot->error = 0;
    snapshot->ready = 1;
    debugtui_pmu_fixture_stop();
}
