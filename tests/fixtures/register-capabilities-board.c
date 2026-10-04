/* Dedicated board-test hook. Build with debug symbols and without optimization,
 * and call only from a test firmware after normal initialization. Configure
 * compiler-specific no-inline/retention flags if needed. Stop at this function
 * before invoking the driver. This hook does not change CPU mode, FPU enablement,
 * MPU selection or PMU controls. The driver never starts or stops the CPU.
 * For Hyp evidence, enter here from firmware already running in Hyp; the hook
 * itself does not create that privilege or change either core's mode.
 */
#include <stdint.h>

volatile uint32_t debug_capability_fixture_ready;

void debug_capability_fixture(void)
{
    debug_capability_fixture_ready = 1;
    for (;;) {
        /* Keep a stable, explicitly selected frame 0 for read-only inspection. */
    }
}
