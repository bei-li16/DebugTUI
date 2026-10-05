/* Dedicated little-endian R52 normal-Hyp fixture. Caller supplies independently
 * verified Debug, GICD and this core's GICR control-page bases and a private slot.
 * No address discovery, bus probe, unlock, acknowledge or peripheral write.
 * Startup must establish static interrupt/debug state independently. Reading
 * a wrong, inaccessible or unpowered address can fault: keep mapping_verified=0
 * until the board configuration and permissions are independently verified.
 * 64-bit MMIO is two sequential 32-bit reads, not an atomic snapshot. */
#include <stdint.h>
struct debugtui_mmio_snapshot { uint32_t ready, error, cpsr, midr; uint64_t raw[24]; };
volatile struct debugtui_mmio_snapshot debugtui_mmio_reference[4];
#define LOAD32(base,offset) (*(volatile const uint32_t *)((uintptr_t)(base)+(offset)))
#define SAVE(n,base,offset) snapshot->raw[n]=LOAD32(base,offset)
#define PAIR(n,base,offset) do { uint32_t lo=LOAD32(base,offset); \
    uint32_t hi=LOAD32(base,(offset)+4); snapshot->raw[n]=(uint64_t)lo|((uint64_t)hi<<32); } while (0)
__attribute__((noinline)) void debugtui_mmio_fixture_stop(void) { __asm__ volatile("" ::: "memory"); }
void debugtui_mmio_capture(volatile struct debugtui_mmio_snapshot *snapshot,
    uint32_t gicd, uint32_t gicr, uint32_t debug, uint32_t mapping_verified)
{
    snapshot->ready=0;snapshot->error=1;
    if (mapping_verified != 1 || (gicd & 3) || (gicr & 3) || (debug & 3) ||
        gicd > UINT32_C(0xffff0000) || gicr > UINT32_C(0xfffe0000) ||
        debug > UINT32_C(0xfffff000)) return;
    __asm__ volatile("mrs %0,cpsr" : "=r"(snapshot->cpsr));
    if ((snapshot->cpsr & 31) != 26) return;
    __asm__ volatile("mrc p15,0,%0,c0,c0,0" : "=r"(snapshot->midr));
    snapshot->error=2;
    if ((snapshot->midr & UINT32_C(0xff0ffff0)) != UINT32_C(0x410fd130)) return;
    SAVE(14,debug,0xd00);
    if (snapshot->raw[14] != snapshot->midr) return;
    SAVE(0,gicd,0);SAVE(1,gicd,4);SAVE(2,gicd,8);
    SAVE(3,gicr,0);SAVE(4,gicr,4);PAIR(5,gicr,8);
    SAVE(6,gicr,0x14);SAVE(7,gicr,0x10080);SAVE(8,gicr,0x10100);
    SAVE(9,gicr,0x10180);SAVE(10,gicr,0x10c00);SAVE(11,gicr,0x10c04);
    SAVE(12,gicr,0x10400);
    snapshot->error=3;
    if ((snapshot->raw[1] & 31) < 1) return; /* INTID32 must be implemented. */
    PAIR(13,gicd,0x6100);
    SAVE(15,debug,0xfe0);SAVE(16,debug,0xfe4);SAVE(17,debug,0xfe8);
    SAVE(18,debug,0xfec);SAVE(19,debug,0xfd0);
    SAVE(20,debug,0xff0);SAVE(21,debug,0xff4);SAVE(22,debug,0xff8);SAVE(23,debug,0xffc);
    snapshot->error=0;snapshot->ready=1;debugtui_mmio_fixture_stop();
}
