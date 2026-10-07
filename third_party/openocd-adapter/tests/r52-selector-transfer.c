/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Actual production transaction, independent instruction words and physical
 * I/O fault injection. This model does not execute ARM instructions. */
#include <assert.h>
#include <stdio.h>
#include "armv8_debugtui_r52.h"

struct fixture {
	unsigned int bank, index, count, operations, fail_at, writes, reads, data_reads;
	unsigned int capacity_reads, status_reads, pc_reads, midr_reads, dscr_reads, barriers;
	uint32_t r0, selector, original, midr, dscr, dspsr, dlr, capacity;
	uint32_t capacity_change, status_change, pc_change, midr_change, dscr_change;
	bool bad_select, bad_restore, bad_scratch, transport_flags;
};
static int step(struct fixture *f) { return ++f->operations == f->fail_at ? -7 : 0; }
static int read_debug(void *context, uint32_t offset, uint32_t *value)
{
	struct fixture *f = context;
	if (step(f)) return -7;
	if (offset == 0x088) {
		*value = f->dscr ^ (++f->dscr_reads == 2 ? f->dscr_change : 0);
		if (f->transport_flags && f->dscr_reads > 1) *value |= 3u << 29;
	} else {
		assert(offset == 0xd00);
		*value = f->midr ^ (++f->midr_reads == 2 ? f->midr_change : 0);
	}
	return 0;
}
static int read_gpr(void *context, unsigned int reg, uint32_t *value)
{
	struct fixture *f = context;
	assert(reg == 0);
	if (step(f)) return -7;
	*value = f->r0;
	return 0;
}
static int write_gpr(void *context, unsigned int reg, uint32_t value)
{
	struct fixture *f = context;
	assert(reg == 0);
	if (step(f)) return -7;
	f->r0 = value ^ (f->bad_scratch && value == 0x11223344 ? 1 : 0);
	return 0;
}
/* R52 TRM PRSELR/HPRSELR, selected BAR/LAR and capacity register tables;
 * ISB is the T32 word consumed by OpenOCD's AArch32 T32_FMTITR path. */
static int execute(void *context, uint32_t opcode)
{
	struct fixture *f = context;
	if (step(f)) return -7;
	if (opcode == 0xee740f15)
		f->r0 = f->dspsr ^ (++f->status_reads == 2 ? f->status_change : 0);
	else if (opcode == 0xee740f35)
		f->r0 = f->dlr ^ (++f->pc_reads == 2 ? f->pc_change : 0);
	else if (opcode == (f->bank ? 0xee900f90u : 0xee100f90u))
		f->r0 = f->capacity ^ (++f->capacity_reads == 2 ? f->capacity_change : 0);
	else if (opcode == (f->bank ? 0xee960f32u : 0xee160f32u)) {
		f->reads++;
		f->r0 = f->selector ^ ((f->bad_select && f->reads == 2) ||
			(f->bad_restore && f->reads == 3) ? 1 : 0);
	} else if (opcode == (f->bank ? 0xee860f32u : 0xee060f32u)) {
		assert(f->r0 < f->count && (f->r0 == f->index || f->r0 == f->original));
		assert(f->barriers > 0);
		f->selector = f->r0;
		f->writes++;
	} else if (opcode == 0xf3bf8f6f) {
		f->barriers++;
	} else {
		bool limit = opcode == (f->bank ? 0xee960f33u : 0xee160f33u);
		assert(limit || opcode == (f->bank ? 0xee960f13u : 0xee160f13u));
		assert(f->selector == f->index && f->reads == 2 && f->barriers > 0);
		f->r0 = (f->bank ? 0x30000000 : 0x20000000) + f->index * 0x10000 +
			(limit ? 0xffc1 : 0x1b);
		f->data_reads++;
	}
	return 0;
}
static struct fixture fresh(unsigned int bank, unsigned int count, unsigned int index, unsigned int original)
{
	return (struct fixture){.bank = bank, .count = count, .index = index,
		.original = original, .selector = original, .r0 = 0x11223344,
		.midr = 0x411fd134, .dscr = 0x01050213, .dspsr = 0xa2000410,
		.dlr = 0x81234568, .capacity = bank ? count : count << 8};
}
static int transfer(struct fixture *f, unsigned int expected,
	struct armv8_debugtui_r52_selector_result *result, bool *uncertain)
{
	struct armv8_debugtui_timer_io io = {
		.core = {.context = f, .read_gpr = read_gpr, .write_gpr = write_gpr, .execute = execute},
		.read_debug = read_debug,
	};
	return armv8_debugtui_r52_select(&io, f->bank, f->index, expected, result, uncertain);
}
int main(void)
{
	unsigned int successes = 0, failures = 0, refusals = 0;
	const unsigned int counts[] = {16, 20, 24};
	for (unsigned int bank = 0; bank < 2; bank++) {
		for (unsigned int c = 0; c < 3; c++) {
			unsigned int count = counts[c];
			for (unsigned int index = 0; index < count; index++) {
				const unsigned int originals[] = {0, index, count-1};
				for (unsigned int o = 0; o < 3; o++) {
					struct fixture f = fresh(bank, count, index, originals[o]);
					struct armv8_debugtui_r52_selector_result result = {.base = 11};
					bool uncertain = true;
					assert(transfer(&f, count, &result, &uncertain) == 0 && !uncertain);
					assert(result.original == originals[o] && result.restored == originals[o]);
					assert(result.base == (bank ? 0x30000000 : 0x20000000) + index * 0x10000 + 0x1b);
					assert(result.limit == result.base - 0x1b + 0xffc1);
					assert(result.proof.capacity == f.capacity && result.proof.dspsr == f.dspsr);
					assert(f.r0 == 0x11223344 && f.selector == originals[o] && f.data_reads == 2);
					assert(f.writes == (index == originals[o] ? 0u : 2u));
					unsigned int operations = f.operations;
					for (unsigned int failure = 1; failure <= operations; failure++) {
						f = fresh(bank, count, index, originals[o]); f.fail_at = failure;
						result = (struct armv8_debugtui_r52_selector_result){.base = 11};
						assert(transfer(&f, count, &result, &uncertain) == -7 && uncertain);
						assert(f.operations == failure && result.base == 11 && result.proof.midr == 0);
						failures++;
					}
					successes++;
				}
			}
			for (unsigned int original = count; original < 34; original++) {
				struct fixture f = fresh(bank, count, 0, original);
				struct armv8_debugtui_r52_selector_result result = {.base = 11}; bool uncertain = true;
				assert(transfer(&f, count, &result, &uncertain) == 6 && !uncertain);
				assert(!f.writes && !f.data_reads && !f.barriers && f.r0 == 0x11223344 && result.base == 11);
				refusals++;
			}
		}
		const unsigned int capacities[] = {0, 1, 15, 16, 20, 24, 25, 255};
		for (unsigned int i = 0; i < 8; i++) {
			struct fixture f = fresh(bank, capacities[i], 0, 1);
			struct armv8_debugtui_r52_selector_result result = {.base = 11}; bool uncertain = true;
			int expected = !capacities[i] && bank ? 4 :
				capacities[i] != 16 && capacities[i] != 20 && capacities[i] != 24 ? 1 :
				capacities[i] != 24 ? 5 : 0;
			assert(transfer(&f, 24, &result, &uncertain) == expected && !uncertain);
			if (expected) assert(!f.reads && !f.writes && !f.data_reads && !f.barriers && result.base == 11);
			assert(f.r0 == 0x11223344 && f.capacity_reads == 2);
			refusals++;
		}
	}
	for (unsigned int mutation = 0; mutation < 16; mutation++) {
		struct fixture f = fresh(1, 20, 19, 2);
		if (mutation == 0) f.bad_scratch = true;
		if (mutation == 1) f.bad_select = true;
		if (mutation == 2) f.bad_restore = true;
		if (mutation == 3) f.status_change = 1u << 26;
		if (mutation == 4) f.pc_change = 4;
		if (mutation == 5) f.midr_change = 0x10;
		if (mutation == 6) f.capacity_change = 1;
		if (mutation >= 7) { const unsigned int bits[] = {8, 15, 16, 18, 10, 6, 24, 12, 7}; f.dscr_change = 1u << bits[mutation-7]; }
		struct armv8_debugtui_r52_selector_result result = {.base = 11}; bool uncertain = false;
		assert(transfer(&f, 20, &result, &uncertain) != 0 && uncertain && result.base == 11);
		if (mutation == 1) assert(f.writes == 1 && !f.data_reads && f.selector == 19);
	}
	for (unsigned int preflight = 0; preflight < 6; preflight++) {
		struct fixture f = fresh(0, 24, 3, 1);
		if (preflight == 0) f.dscr = 0x01050013;
		if (preflight == 1) f.dscr = 0x01050113;
		if (preflight == 2) f.dscr |= 1u << 15;
		if (preflight == 3) f.midr = 0x411fd143;
		if (preflight == 4) f.dscr &= ~(1u << 24);
		if (preflight == 5) f.dscr |= 1u << 6;
		struct armv8_debugtui_r52_selector_result result = {.base = 11}; bool uncertain = false;
		assert(transfer(&f, 24, &result, &uncertain) != 0 && uncertain == (preflight == 5));
		assert(!f.status_reads && !f.writes && !f.data_reads && result.base == 11);
	}
	struct fixture f = fresh(1, 20, 19, 2); f.transport_flags = true;
	struct armv8_debugtui_r52_selector_result result = {0}; bool uncertain = true;
	assert(transfer(&f, 20, &result, &uncertain) == 0 && !uncertain);
	f = fresh(1, 0, 3, 1); f.status_change = 1;
	assert(transfer(&f, 24, &result, &uncertain) != 0 && uncertain && !f.writes);
	for (unsigned int invalid = 0; invalid < 3; invalid++) {
		f = fresh(invalid == 0 ? 2 : 0, 24, invalid == 1 ? 24 : 0, 1);
		assert(transfer(&f, invalid == 2 ? 0 : 24, &result, &uncertain) == 1 && !uncertain && !f.operations);
	}
	printf("PASS: %u selector success cases, %u immediate-stop physical fault points, %u capacity/original-selector cases; preserved R0, DSPSR, DLR and selector; no mode/control writes\n", successes, failures, refusals);
	return 0;
}
