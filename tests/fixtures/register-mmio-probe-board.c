/* Independent normal-Hyp little-endian baseline, static verified board only.
 * Private per-core slots; no discovery, unlock, control or peripheral writes.
 * Call only after independently verifying Device mappings, bus permission,
 * power, all three owners and coherent baseline RAM. Host never calls it. */
#include <stdint.h>
struct debugtui_mmio_probe_snapshot { uint32_t ready,error,cpsr,midr; uint64_t raw[21]; };
volatile struct debugtui_mmio_probe_snapshot debugtui_mmio_probe_reference[4];
#define LOAD32(base,offset) (*(volatile const uint32_t *)((uintptr_t)(base)+(offset)))
#define SAVE(n,base,offset) snapshot->raw[n]=LOAD32(base,offset)
#define PAIR(n,base,offset) do { uint32_t lo=LOAD32(base,offset); uint32_t hi=LOAD32(base,(offset)+4); snapshot->raw[n]=(uint64_t)lo|((uint64_t)hi<<32); } while (0)
__attribute__((noinline)) void debugtui_mmio_probe_fixture_stop(void) { __asm__ volatile("" ::: "memory"); }
void debugtui_mmio_probe_capture(volatile struct debugtui_mmio_probe_snapshot *snapshot,
 uint32_t gicd,uint32_t gicr,uint32_t debug,uint32_t mapping_verified)
{
 snapshot->ready=0;snapshot->error=1;
 if (mapping_verified!=1 || (gicd&3) || (gicr&7) || (debug&3) || gicd>UINT32_C(0xffff0000) || gicr>UINT32_C(0xffff0000) || debug>UINT32_C(0xfffff000)) return;
 __asm__ volatile("mrs %0,cpsr" : "=r"(snapshot->cpsr));
 if ((snapshot->cpsr&31)!=26) return;
 __asm__ volatile("mrc p15,0,%0,c0,c0,0" : "=r"(snapshot->midr));
 snapshot->error=2;
 if ((snapshot->midr&UINT32_C(0xff0ffff0))!=UINT32_C(0x410fd130)) return;
 SAVE(0,debug,0xd00);if(snapshot->raw[0]!=snapshot->midr)return;
 SAVE(1,debug,0xff0);SAVE(2,debug,0xff4);SAVE(3,debug,0xff8);SAVE(4,debug,0xffc);
 SAVE(5,debug,0xfa8);SAVE(6,debug,0xfac);SAVE(7,debug,0xd28);SAVE(8,debug,0xd2c);
 SAVE(9,gicd,8);SAVE(10,gicd,0xfff0);SAVE(11,gicd,0xfff4);SAVE(12,gicd,0xfff8);SAVE(13,gicd,0xfffc);SAVE(14,gicd,4);
 SAVE(15,gicr,4);SAVE(16,gicr,0xfff0);SAVE(17,gicr,0xfff4);SAVE(18,gicr,0xfff8);SAVE(19,gicr,0xfffc);PAIR(20,gicr,8);
 snapshot->error=0;snapshot->ready=1;debugtui_mmio_probe_fixture_stop();
}
