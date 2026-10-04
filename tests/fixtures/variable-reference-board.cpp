/* Dedicated C++ fixture in declared per-core RAM, -g -O0.
 * Stop at debug_reference_fixture. Caller frame 1 has local_reference.
 * Do not run target helper functions from the debugger.
 */
#include <stdint.h>
#if defined(__GNUC__) || defined(__clang__)
#define REFERENCE_NOINLINE __attribute__((noinline))
#else
#define REFERENCE_NOINLINE
#endif
struct ReferenceCells { uint32_t before; uint32_t value; uint32_t after; } reference_cells={0x12345678,7,0x87654321};
uint32_t &reference_value=reference_cells.value;
uint32_t &&rvalue_reference=static_cast<uint32_t&&>(reference_cells.value);
const uint32_t reference_constant=11;
const uint32_t &const_reference=reference_constant;
volatile uint32_t reference_volatile=13;
volatile uint32_t &volatile_reference=reference_volatile;
uint32_t reference_fixture_calls;
volatile uint32_t reference_fixture_sink;
extern "C" REFERENCE_NOINLINE int reference_fixture_side_effect(void) { ++reference_fixture_calls; return 42; }
extern "C" REFERENCE_NOINLINE void debug_reference_fixture(void) { reference_fixture_sink=reference_cells.value; }
extern "C" REFERENCE_NOINLINE void debug_reference_fixture_caller(void) {
    uint32_t local=17;
    uint32_t &local_reference=local;
    debug_reference_fixture();
    reference_fixture_sink=local_reference;
}
/* A 32-bit Cortex-R52 compiler may lack a scalar __int128 type.
 * Never substitute a pair/aggregate or a CPU vector for a scalar writer case.
 */
#if defined(__SIZEOF_INT128__)
struct WideValue { uint64_t before; alignas(16) unsigned __int128 value; uint64_t after; } wide_value={UINT64_C(0x1122334455667788),1,UINT64_C(0x8877665544332211)};
__int128 signed_wide_value=1;
const uint32_t reference_fixture_has_int128=1;
#else
const uint32_t reference_fixture_has_int128=0;
#endif
