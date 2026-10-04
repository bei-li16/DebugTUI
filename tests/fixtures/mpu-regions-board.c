/* Dedicated firmware hook for REG-H04 MPU/MAIR overview.
 * Configure MPU independently in the board's startup code, record every raw
 * region and MAIR expectation in mpu-regions-board.example.json, and stop in
 * physical frame 0 here. Run core0/core1 separately, then Scope All with a peer.
 * Do not copy the example's software-model addresses into a real MPU setup.
 * This hook does not change selectors, CPU mode, MPU, MAIR, PMU or CP15BEN.
 * Build with debug symbols and compiler-specific no-inline/optimization off.
 */
#include <stdint.h>
volatile uint32_t debug_mpu_fixture_ready;
void debug_mpu_fixture(void)
{
    debug_mpu_fixture_ready = 1;
    for (;;) { /* Capture independent MPU setup evidence before stopping here. */ }
}
