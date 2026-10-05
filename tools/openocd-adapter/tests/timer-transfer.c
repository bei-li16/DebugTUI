/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Execute the production transaction, modeling only its physical I/O. */
#include <assert.h>
#include <stdio.h>
#include "armv8_debugtui_timer.h"

/* Independently transcribed T32 words, Rt=0/Rt2=1, R52 TRM Table 11-1. */
static const uint32_t encodings[] = {
	0xee1e0f10, 0xee1e0f11, 0xee1e0f12, 0xee1e0f32,
	0xee1e0f13, 0xee1e0f33, 0xee9e0f11, 0xee9e0f12, 0xee9e0f32,
	0xec510f0e, 0xec510f1e, 0xec510f2e, 0xec510f3e, 0xec510f4e, 0xec510f6e,
};
struct fixture {
	uint32_t gpr[2], dscr, midr, dspsr, dlr;
	unsigned int operations, fail_at, dscr_reads, midr_reads, status_reads, pc_reads;
	unsigned int timer_reads, index, corrupt_restore;
	uint32_t dscr_change, midr_change, status_change, pc_change;
	bool moving;
	uint64_t counter;
};
static int step(struct fixture *f)
{
	return ++f->operations == f->fail_at ? -7 : 0;
}
static int read_debug(void *context, uint32_t offset, uint32_t *value)
{
	struct fixture *f = context;
	if (step(f))
		return -7;
	if (offset == 0x088)
		*value = f->dscr ^ (++f->dscr_reads == 3 ? f->dscr_change : 0);
	else {
		assert(offset == 0xd00);
		*value = f->midr ^ (++f->midr_reads == 2 ? f->midr_change : 0);
	}
	return 0;
}
static int read_gpr(void *context, unsigned int reg, uint32_t *value)
{
	struct fixture *f = context;
	assert(reg < 2);
	if (step(f))
		return -7;
	*value = f->gpr[reg];
	/* The timer advances after the instruction has latched both destination
	 * words, including between the separate physical R0/R1 transport reads. */
	if (f->moving && f->timer_reads)
		f->counter++;
	return 0;
}
static int write_gpr(void *context, unsigned int reg, uint32_t value)
{
	struct fixture *f = context;
	assert(reg < 2);
	if (step(f))
		return -7;
	f->gpr[reg] = value ^ (f->corrupt_restore == reg + 1 ? 1 : 0);
	return 0;
}
static uint64_t timer_value(unsigned int index)
{
	return index < 9 ? UINT64_C(0xfffffed0) + index : UINT64_C(0xfedcba9876543210) + index;
}
static int execute(void *context, uint32_t opcode)
{
	struct fixture *f = context;
	if (step(f))
		return -7;
	if (opcode == UINT32_C(0xee740f15))
		f->gpr[0] = f->dspsr ^ (++f->status_reads == 2 ? f->status_change : 0);
	else if (opcode == UINT32_C(0xee740f35))
		f->gpr[0] = f->dlr ^ (++f->pc_reads == 2 ? f->pc_change : 0);
	else {
		assert(opcode == encodings[f->index]);
		f->timer_reads++;
		uint64_t value = f->moving ? f->counter : timer_value(f->index);
		f->gpr[0] = value;
		if (f->index >= 9)
			f->gpr[1] = value >> 32;
	}
	return 0;
}
static struct fixture fresh(unsigned int index, unsigned int el)
{
	return (struct fixture){.gpr = {0x11223344, 0x55667788},
		.dscr = (1u << 24) | (el << 8), .midr = 0x411fd134,
		.dspsr = 0xa200041a, .dlr = 0x81234568, .index = index};
}
static int transfer(struct fixture *f, struct armv8_debugtui_timer_result *result, bool *uncertain)
{
	struct armv8_debugtui_timer_io io = {
		.core = {.context = f, .read_gpr = read_gpr, .write_gpr = write_gpr, .execute = execute},
		.read_debug = read_debug,
	};
	return armv8_debugtui_timer_transfer(&io, &armv8_debugtui_timers[f->index], result, uncertain);
}
int main(void)
{
	unsigned int fault_points = 0, legal = 0, refusals = 0;
	const char *invalid[] = {"CNTPCT", "cntpct; reset", "cntp", "cntp_ctl 0", ""};
	for (unsigned int i = 0; i < sizeof(invalid) / sizeof(invalid[0]); i++)
		assert(!armv8_debugtui_timer_find(invalid[i]));
	for (unsigned int index = 0; index < 15; index++) {
		assert(armv8_debugtui_timer_find(armv8_debugtui_timers[index].name) == &armv8_debugtui_timers[index]);
		struct fixture f = fresh(index, 2);
		struct armv8_debugtui_timer_result result = {.value = 11};
		bool uncertain = true;
		assert(transfer(&f, &result, &uncertain) == 0 && !uncertain);
		assert(result.value == timer_value(index) && f.timer_reads == 1);
		assert(result.midr == f.midr && result.dscr == f.dscr && result.dspsr == f.dspsr && result.dlr == f.dlr);
		assert(f.gpr[0] == 0x11223344 && f.gpr[1] == 0x55667788);
		unsigned int count = f.operations;
		assert(count == (index < 9 ? 30u : 34u));
		for (unsigned int failure = 1; failure <= count; failure++) {
			f = fresh(index, 2); f.fail_at = failure;
			result = (struct armv8_debugtui_timer_result){.value = 11};
			assert(transfer(&f, &result, &uncertain) == -7 && uncertain);
			assert(f.operations == failure && result.value == 11 && result.midr == 0);
			fault_points++;
		}
		/* Permission decisions use actual Debug EL, not stopped DSPSR.M. */
		for (unsigned int el = 0; el < 2; el++) {
			for (unsigned int hdd = 0; hdd < 2; hdd++) {
				f = fresh(index, el); f.dscr |= hdd << 16;
				int expected = el == 0 ? 3 : 0;
				if (index == 1 && el == 0) expected = 2;
				if (index == 2 || index == 3 || index == 9 || index == 11) expected = 3;
				if (index == 6 || index == 7 || index == 8 || index == 13 || index == 14) expected = 2;
				result = (struct armv8_debugtui_timer_result){.value = 11};
				assert(transfer(&f, &result, &uncertain) == expected && !uncertain);
				if (expected) {
					assert(f.operations == 2 && f.timer_reads == 0 && f.status_reads == 0 && result.value == 11);
					refusals++;
				} else {
					assert(f.timer_reads == 1 && result.value == timer_value(index));
					legal++;
				}
				assert(f.gpr[0] == 0x11223344 && f.gpr[1] == 0x55667788);
			}
		}
	}
	for (unsigned int mutation = 0; mutation < 11; mutation++) {
		struct fixture f = fresh(10, 2);
		if (mutation < 2) f.corrupt_restore = mutation + 1;
		if (mutation == 2) f.status_change = 1u << 26;
		if (mutation == 3) f.pc_change = 4;
		if (mutation == 4) f.midr_change = 1u << 20;
		if (mutation == 5) f.dscr_change = 1u << 8;
		if (mutation == 6) f.dscr_change = 1u << 16;
		if (mutation == 7) f.dscr_change = 1u << 18;
		if (mutation == 8) f.dscr_change = 1u << 10;
		if (mutation == 9) f.dscr_change = 1u << 6;
		if (mutation == 10) f.dscr_change = 1u << 24;
		struct armv8_debugtui_timer_result result = {.value = 11};
		bool uncertain = false;
		assert(transfer(&f, &result, &uncertain) != 0 && uncertain && result.value == 11);
	}
	for (unsigned int invalid_state = 0; invalid_state < 5; invalid_state++) {
		struct fixture f = fresh(10, 2);
		if (invalid_state == 0) f.midr = 0x411fd143;
		if (invalid_state == 1) f.dscr |= 1u << 12;
		if (invalid_state == 2) f.dscr |= 1u << 16;
		if (invalid_state == 3) f.dscr |= 1u << 8;
		if (invalid_state == 4) f.dscr |= 1u << 6;
		struct armv8_debugtui_timer_result result = {.value = 11};
		bool uncertain = false;
		assert(transfer(&f, &result, &uncertain) != 0);
		assert(uncertain == (invalid_state == 4) && result.value == 11 && f.timer_reads == 0);
	}
	const uint64_t edges[] = {
		0, 1, UINT64_C(0x00000000ffffffff), UINT64_C(0x0000000100000000),
		UINT64_C(0x7fffffffffffffff), UINT64_C(0x8000000000000000),
		UINT64_C(0xffffffff00000000), UINT64_MAX,
	};
	unsigned int moving_reads = 0;
	for (unsigned int index = 9; index < 15; index++) {
		for (unsigned int edge = 0; edge < sizeof(edges) / sizeof(edges[0]); edge++) {
			struct fixture f = fresh(index, 2);
			f.moving = true; f.counter = edges[edge];
			struct armv8_debugtui_timer_result result = {.value = 11};
			bool uncertain = true;
			assert(transfer(&f, &result, &uncertain) == 0 && !uncertain);
			assert(result.value == edges[edge] && f.timer_reads == 1);
			assert(f.counter != edges[edge]);
			assert(f.gpr[0] == 0x11223344 && f.gpr[1] == 0x55667788);
			moving_reads++;
		}
	}
	printf("PASS: 15 Timer encodings, %u fault points, %u legal EL1/HDD views, %u permission refusals, DSPSR/DLR/identity/EL preservation\n",
		fault_points, legal, refusals);
	printf("PASS: %u moving 64-bit reads, latched words across low-word carry and full counter wrap\n", moving_reads);
	return 0;
}
