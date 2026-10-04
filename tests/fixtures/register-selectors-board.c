/* Call from dedicated test firmware with debug symbols and optimization off.
 * Stop in this function's physical frame 0 before running the selector driver.
 * Firmware must already provide the declared MPU/PMU values and CPU mode;
 * this hook does not change MPU selectors/configuration, PMU or CP15BEN.
 * For EL2 tests, enter here while already in Hyp. Run each selected core's
 * case separately, optionally recording the other paused core as peer_core.
 */
#include <stdint.h>

volatile uint32_t debug_selector_fixture_ready;

void debug_selector_fixture(void)
{
    debug_selector_fixture_ready = 1;
    for (;;) {
        /* Retain a stable frame with compiler-specific no-inline settings. */
    }
}
