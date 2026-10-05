#include <stdint.h>

// Put these objects in ordinary private RAM of the explicitly selected core.
// This fixture neither changes CPU modes nor accesses device registers.
struct BitfieldCells {
    uint32_t before;
    unsigned low : 5;
    signed middle : 6;
    unsigned neighbour : 7;
    unsigned reserved : 14;
    uint32_t after;
};
BitfieldCells bitfield_cells = {0x12345678, 3, -2, 0x55, 0x2abc, 0x87654321};
volatile BitfieldCells bitfield_volatile;
const BitfieldCells bitfield_const = {1, 2, -3, 4, 5, 6};
struct __attribute__((packed)) PackedBitfields {
    uint8_t before;
    unsigned low : 5;
    signed middle : 6;
    unsigned neighbour : 7;
    uint8_t after;
};
PackedBitfields bitfield_packed = {0x12, 3, -2, 0x55, 0x87};
struct WideBitfields {
    uint64_t before;
    unsigned long long full : 64;
    signed long long signed_value : 63;
    unsigned spare : 1;
    uint64_t after;
};
WideBitfields bitfield_wide = {0x1122334455667788ULL, 1, -2, 1, 0x8877665544332211ULL};
unsigned bitfield_calls = 0;
uint32_t bitfield_sink = 0;
extern "C" void debug_bitfield_fixture() {
    bitfield_sink = bitfield_cells.before;
}
#ifdef DEBUGTUI_BITFIELD_HOST_MAIN
int main() { debug_bitfield_fixture(); return 0; }
#endif
