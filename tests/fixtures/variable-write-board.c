/* Link fixture into per-core private RAM/TCM for the example's core owner.
 * For shared RAM, declare chip/cluster ownership and halt every related core.
 * Build with debug information and -O0. Set a breakpoint at
 * debug_write_fixture; the driver connects to an already halted fixture.
 * Do not declare the writable objects volatile or const.
 */
#include <stdint.h>
#if defined(__GNUC__) || defined(__clang__)
#define FIXTURE_NOINLINE __attribute__((noinline))
#else
#define FIXTURE_NOINLINE
#endif
struct VariableWriteFixture {
    uint32_t value;
    uint32_t before;
    uint32_t after;
    int32_t items[3];
    uint64_t wide;
    float single;
    double real;
};
struct VariableWriteFixture fixture = {
    0x13579bdf, 0x11223344, 0x55667788, {10,11,12},
    UINT64_C(0x123456789abcdef0), 1.5f, 2.5
};
volatile uint32_t variable_write_fixture_sink;
FIXTURE_NOINLINE void debug_write_fixture(void) {
    variable_write_fixture_sink = fixture.value;
}
/* Stop in debug_write_fixture; select frame 1 for the initialized local.
 * A separate case uses pane=locals, expression=local_fixture, path=[0],
 * probe=local_fixture.value, path_expression=(local_fixture).value,
 * frame=1, frame_function=debug_write_fixture_caller, and its own sentinels.
 */
FIXTURE_NOINLINE void debug_write_fixture_caller(void) {
    struct VariableWriteFixture local_fixture = fixture;
    debug_write_fixture();
    variable_write_fixture_sink = local_fixture.value;
}
