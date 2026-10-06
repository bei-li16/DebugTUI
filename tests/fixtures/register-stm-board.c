/* Independent STM-500 firmware baseline. Compile only on the host; board
 * integration supplies a verified powered/permitted control mapping and a
 * static fixture. Never invoke this function from a debugger evaluation. */
#include <stdint.h>
#include <stddef.h>
struct debugtui_stm_snapshot { uint32_t ready, core_tag, raw[6]; };
volatile struct debugtui_stm_snapshot debugtui_stm_reference[4];
int debugtui_stm_capture(unsigned core, volatile const uint32_t *control) {
    static const unsigned offsets[6] = {0xe80,0xe00,0xe60,0xe64,0xea0,0xfc8};
    if (core >= 4 || !control || ((uintptr_t)control & 0xfff)) return -1;
    debugtui_stm_reference[core].ready = 0;
    if (control[0xff0/4]!=13 || control[0xff4/4]!=0x90 ||
        control[0xff8/4]!=5 || control[0xffc/4]!=0xb1 ||
        control[0xfbc/4]!=0x47710a63 || control[0xfcc/4]!=0x63 ||
        control[0xfe0/4]!=0x63 || control[0xfe4/4]!=0xb9 ||
        (control[0xfe8/4]&15)!=11 || control[0xfd0/4]!=4 ||
        (control[0xea4/4]&4)!=0) return -2;
    debugtui_stm_reference[core].core_tag = core;
    for (unsigned i=0;i<6;i++) debugtui_stm_reference[core].raw[i]=control[offsets[i]/4];
    debugtui_stm_reference[core].ready = 1;
    return 0;
}
