/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Production transaction with modeled physical I/O; no ARM execution claim. */
#include <assert.h>
#include <stdio.h>
#include "armv8_debugtui_r52.h"

/* R52 TRM 100026_0104_01_en: independently transcribed MRC words, Rt=0.
 * Scalars: §§4.3.1, .3, .29, .116-.120, .147-.149, .194-.200.
 * Region BARs: Tables 4-117/4-196; LAR changes opc2 by one. */
static const char *names[] = {"midr", "mpidr", "sctlr", "hsctlr", "cpacr", "hcr",
	"mpuir", "hmpuir", "prselr", "hprselr", "hprenr", "mair0", "mair1", "hmair0", "hmair1"};
static const uint32_t scalar_words[] = {
	0xee100f10, 0xee100fb0, 0xee110f10, 0xee910f10, 0xee110f50, 0xee910f11,
	0xee100f90, 0xee900f90, 0xee160f32, 0xee960f32, 0xee960f31,
	0xee1a0f12, 0xee1a0f32, 0xee9a0f12, 0xee9a0f32,
};
static const uint32_t region_bar_words[] = {
	0xee160f18, 0xee160f98, 0xee160f19, 0xee160f99, 0xee160f1a, 0xee160f9a,
	0xee160f1b, 0xee160f9b, 0xee160f1c, 0xee160f9c, 0xee160f1d, 0xee160f9d,
	0xee160f1e, 0xee160f9e, 0xee160f1f, 0xee160f9f,
	0xee360f18, 0xee360f98, 0xee360f19, 0xee360f99, 0xee360f1a, 0xee360f9a,
	0xee360f1b, 0xee360f9b,
};
struct fixture {
	struct armv8_debugtui_r52_register reg;
	uint32_t word, r0, midr, dscr, dspsr, dlr, capacity[2];
	unsigned int operations, fail_at, data_reads, capacity_reads, dscr_reads, midr_reads, status_reads, pc_reads;
	uint32_t dscr_change, midr_change, status_change, pc_change, capacity_change, data_change;
	bool corrupt_restore, transport_flags;
};
static int step(struct fixture *f) { return ++f->operations == f->fail_at ? -7 : 0; }
static int read_debug(void *context, uint32_t offset, uint32_t *value)
{
	struct fixture *f = context;
	if (step(f)) return -7;
	if (offset == 0x088) {
		*value = f->dscr ^ (++f->dscr_reads == 3 ? f->dscr_change : 0);
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
	f->r0 = value ^ (f->corrupt_restore ? 1 : 0);
	return 0;
}
static uint32_t data_value(const struct fixture *f)
{
	if (f->word == 0xee100f10) return f->midr;
	if (f->word == 0xee100f90) return f->capacity[0];
	if (f->word == 0xee900f90) return f->capacity[1];
	return f->word ^ 0x12345678;
}
static int execute(void *context, uint32_t opcode)
{
	struct fixture *f = context;
	if (step(f)) return -7;
	if (opcode == 0xee740f15)
		f->r0 = f->dspsr ^ (++f->status_reads == 2 ? f->status_change : 0);
	else if (opcode == 0xee740f35)
		f->r0 = f->dlr ^ (++f->pc_reads == 2 ? f->pc_change : 0);
	else if (f->reg.bank >= 0 && (opcode == 0xee100f90 || opcode == 0xee900f90)) {
		assert(opcode == (f->reg.bank ? 0xee900f90u : 0xee100f90u));
		f->r0 = f->capacity[f->reg.bank] ^ (++f->capacity_reads == 2 ? f->capacity_change : 0);
	} else {
		assert(opcode == f->word); /* No selector, mode, enable or control MCR. */
		f->data_reads++;
		f->r0 = data_value(f) ^ f->data_change;
	}
	return 0;
}
static struct fixture fresh(const char *name, uint32_t word)
{
	struct fixture f = {.word = word, .r0 = 0x11223344, .midr = 0x411fd134,
		.dscr = 0x01050213, .dspsr = 0xa2000410, .dlr = 0x81234568,
		.capacity = {0x1800, 0x18}};
	assert(armv8_debugtui_r52_find(name, &f.reg));
	assert(ARMV4_5_MRC(15, f.reg.op1, 0, f.reg.crn, f.reg.crm, f.reg.op2) == word);
	return f;
}
static int transfer(struct fixture *f, struct armv8_debugtui_r52_result *result, bool *uncertain)
{
	struct armv8_debugtui_timer_io io = {
		.core = {.context = f, .read_gpr = read_gpr, .write_gpr = write_gpr, .execute = execute},
		.read_debug = read_debug,
	};
	return armv8_debugtui_r52_transfer(&io, &f->reg, result, uncertain);
}
static unsigned int exercise(const char *name, uint32_t word)
{
	struct fixture f = fresh(name, word);
	struct armv8_debugtui_r52_result result = {.value = 11};
	bool uncertain = true;
	assert(transfer(&f, &result, &uncertain) == 0 && !uncertain);
	assert(result.value == data_value(&f) && result.midr == f.midr && result.dscr == f.dscr);
	assert(result.dspsr == f.dspsr && result.dlr == f.dlr && f.data_reads == 1 && f.r0 == 0x11223344);
	unsigned int count = f.operations;
	for (unsigned int failure = 1; failure <= count; failure++) {
		f = fresh(name, word); f.fail_at = failure;
		result = (struct armv8_debugtui_r52_result){.value = 11};
		assert(transfer(&f, &result, &uncertain) == -7 && uncertain);
		assert(f.operations == failure && result.value == 11 && result.midr == 0);
	}
	for (unsigned int el = 0; el < 2; el++) {
		f = fresh(name, word); f.dscr = 0x01050013 | el << 8;
		result = (struct armv8_debugtui_r52_result){.value = 11};
		assert(transfer(&f, &result, &uncertain) == (f.reg.op1 >= 4 ? 2 : 3) && !uncertain);
		assert(f.operations == 2 && !f.data_reads && !f.status_reads && f.r0 == 0x11223344 && result.value == 11);
	}
	return count;
}
int main(void)
{
	unsigned int fault_points = 0, routes = 0, capacity_cases = 0;
	for (unsigned int i = 0; i < 15; i++) { fault_points += exercise(names[i], scalar_words[i]); routes++; }
	for (unsigned int bank = 0; bank < 2; bank++) {
		for (unsigned int index = 0; index < 24; index++) {
			for (unsigned int limit = 0; limit < 2; limit++) {
				char name[16]; snprintf(name, sizeof(name), "%spr%car%u", bank ? "h" : "", limit ? 'l' : 'b', index);
				uint32_t word = region_bar_words[index] + (bank ? 0x800000 : 0) + (limit ? 0x20 : 0);
				fault_points += exercise(name, word); routes++;
				const unsigned int counts[] = {0, 16, 20, 24, 1, 15, 25, 255};
				for (unsigned int c = 0; c < 8; c++) {
					struct fixture f = fresh(name, word); unsigned int count = counts[c];
					f.capacity[bank] = bank ? count : count << 8;
					/* The other bank has a different valid capacity: never substitute it. */
					f.capacity[!bank] = bank ? 0x1000 : 0x10;
					int expected = count == 0 && bank ? 4 : count != 16 && count != 20 && count != 24 ? 1 : index >= count ? 4 : 0;
					struct armv8_debugtui_r52_result result = {.value = 11}; bool uncertain = true;
					assert(transfer(&f, &result, &uncertain) == expected && !uncertain);
					assert(f.r0 == 0x11223344 && f.data_reads == (expected ? 0u : 1u) && f.capacity_reads == 2);
					if (expected) assert(result.value == 11 && result.midr == 0);
					capacity_cases++;
				}
			}
		}
	}
	struct armv8_debugtui_r52_register unused;
	const char *invalid[] = {"MIDR", "prbar24", "hprlar24", "prbar00", "prlar-1", "hcr; reset", "pmselr", "bpiall", "", "sctlr 0"};
	for (unsigned int i = 0; i < sizeof(invalid)/sizeof(invalid[0]); i++) assert(!armv8_debugtui_r52_find(invalid[i], &unused));
	for (unsigned int mutation = 0; mutation < 13; mutation++) {
		struct fixture f = fresh("hprbar19", 0xeeb60f99);
		if (mutation == 0) f.corrupt_restore = true;
		if (mutation == 1) f.status_change = 1u << 26;
		if (mutation == 2) f.pc_change = 4;
		if (mutation == 3) f.midr_change = 0x10;
		if (mutation == 4) f.capacity_change = 4;
		if (mutation >= 5) { const unsigned int bits[] = {8, 15, 16, 18, 10, 6, 24, 12}; f.dscr_change = 1u << bits[mutation-5]; }
		struct armv8_debugtui_r52_result result = {.value = 11}; bool uncertain = false;
		assert(transfer(&f, &result, &uncertain) != 0 && uncertain && result.value == 11);
	}
	for (unsigned int invalid_state = 0; invalid_state < 4; invalid_state++) {
		struct fixture f = fresh("sctlr", 0xee110f10);
		if (invalid_state == 0) f.dscr |= 1u << 15;
		if (invalid_state == 1) f.midr = 0x411fd143;
		if (invalid_state == 2) f.dscr &= ~(1u << 24);
		if (invalid_state == 3) f.dscr |= 1u << 6;
		struct armv8_debugtui_r52_result result = {.value = 11}; bool uncertain = false;
		assert(transfer(&f, &result, &uncertain) != 0 && uncertain == (invalid_state == 3));
		assert(!f.data_reads && !f.status_reads && result.value == 11);
	}
	/* Capacity refusal still verifies the instruction's restoration boundary. */
	struct fixture f = fresh("hprbar19", 0xeeb60f99); f.capacity[1] = 0; f.status_change = 1;
	struct armv8_debugtui_r52_result result = {.value = 11}; bool uncertain = false;
	assert(transfer(&f, &result, &uncertain) < 0 && uncertain && !f.data_reads && result.value == 11);
	f = fresh("sctlr", 0xee110f10); f.transport_flags = true;
	assert(transfer(&f, &result, &uncertain) == 0 && !uncertain && f.r0 == 0x11223344);
	f = fresh("midr", 0xee100f10); f.data_change = 0x10;
	result = (struct armv8_debugtui_r52_result){.value = 11};
	assert(transfer(&f, &result, &uncertain) < 0 && uncertain && result.value == 11);
	assert(f.data_reads == 1 && f.status_reads == 1 && f.r0 == 0x11223344);
	printf("PASS: %u reviewed R52 routes, %u physical fault points, %u independent-bank capacity cases; EL refusal sends no CPU instruction; no selector/control writes\n", routes, fault_points, capacity_cases);
	return 0;
}
