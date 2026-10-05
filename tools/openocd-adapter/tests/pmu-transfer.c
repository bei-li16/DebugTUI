/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Actual production transaction; model only physical register transport. */
#include <assert.h>
#include <stdio.h>
#include "armv8_debugtui_pmu.h"

/* Independently transcribed AArch32 read instructions, R0 and MRRC R0/R1. */
static const uint32_t opcodes[] = {
	0xee190f1c, 0xee190f3c, 0xee190f5c, 0xee190f7c, 0xee190fbc,
	0xee190fdc, 0xee190ffc, 0xee190f3d, 0xee190f5d, 0xee190f1e,
	0xee190f3e, 0xee190f5e, 0xee190f7e, 0xee1e0fff,
	0xee1e0f18, 0xee1e0f38, 0xee1e0f58, 0xee1e0f78,
	0xee1e0f1c, 0xee1e0f3c, 0xee1e0f5c, 0xee1e0f7c, 0xec510f09,
};
static const uint32_t proof_opcodes[] = {
	0xee740f15, 0xee740f35, 0xee100f51, 0xee190f1c, 0xee910f31, 0xee190fbc,
};
struct fixture {
	uint32_t gpr[2], midr, dscr, proof[6];
	unsigned int operations, fail_at, injected, index, data_reads, debug_reads;
	unsigned int corrupt_gpr, changed_proof;
	uint32_t dscr_change, midr_change;
	bool moving;
	uint64_t value;
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
	f->debug_reads++;
	assert(offset == 0x088 || offset == 0xd00);
	*value = offset == 0x088 ? f->dscr ^ (f->debug_reads == 5 ? f->dscr_change : 0) :
		f->midr ^ (f->debug_reads == 4 ? f->midr_change : 0);
	return 0;
}
static int read_gpr(void *context, unsigned int reg, uint32_t *value)
{
	struct fixture *f = context;
	assert(reg < 2);
	if (step(f))
		return -7;
	*value = f->gpr[reg];
	if (f->moving && f->data_reads)
		f->value++;
	return 0;
}
static int write_gpr(void *context, unsigned int reg, uint32_t value)
{
	struct fixture *f = context;
	assert(reg < 2);
	if (step(f))
		return -7;
	f->gpr[reg] = value ^ (f->corrupt_gpr == reg + 1 ? 1 : 0);
	return 0;
}
static int execute(void *context, uint32_t opcode)
{
	struct fixture *f = context;
	if (step(f))
		return -7;
	/* No MCR, mode switch, PMU enable/reset or selector write is accepted. */
	if (f->injected == 6) {
		assert(opcode == opcodes[f->index]);
		f->data_reads++;
		uint64_t value = f->index == 0 ? f->proof[3] : f->index == 4 ? f->proof[5] : f->value;
		f->gpr[0] = value;
		if (f->index == 22)
			f->gpr[1] = value >> 32;
	} else {
		unsigned int index = f->injected < 6 ? f->injected : f->injected - 7;
		assert(index < 6 && opcode == proof_opcodes[index]);
		f->gpr[0] = f->proof[index] ^ (f->data_reads && f->changed_proof == index + 1 ? 1 : 0);
	}
	f->injected++;
	return 0;
}
static struct fixture fresh(unsigned int index)
{
	return (struct fixture){.gpr = {0x11223344, 0x55667788}, .midr = 0x411fd134,
		.dscr = 0x01000200, .proof = {0xa2000410, 0x81234568, 0x03010066,
			0x41132048, 0x00400e02, 3}, .index = index,
		.value = index == 22 ? UINT64_C(0xfedcba9876543210) : UINT64_C(0xf1234500) + index};
}
static int transfer(struct fixture *f, struct armv8_debugtui_pmu_result *result, bool *uncertain)
{
	struct armv8_debugtui_timer_io io = {
		.core = {.context = f, .read_gpr = read_gpr, .write_gpr = write_gpr, .execute = execute},
		.read_debug = read_debug,
	};
	return armv8_debugtui_pmu_transfer(&io, &armv8_debugtui_pmu_registers[f->index], result, uncertain);
}
int main(void)
{
	assert(sizeof(opcodes) / sizeof(opcodes[0]) == 23);
	assert(sizeof(armv8_debugtui_pmu_registers) / sizeof(armv8_debugtui_pmu_registers[0]) == 23);
	for (unsigned int i = 0; i < 23; i++)
		assert(armv8_debugtui_pmu_find(armv8_debugtui_pmu_registers[i].name) == &armv8_debugtui_pmu_registers[i]);
	assert(!armv8_debugtui_pmu_find("pmswinc") && !armv8_debugtui_pmu_find("pmevcntr4"));
	unsigned int failures = 0, refused = 0, changed = 0;
	struct armv8_debugtui_pmu_result sentinel;
	memset(&sentinel, 0xa5, sizeof(sentinel));
	for (unsigned int index = 0; index < 23; index++) {
		struct fixture f = fresh(index);
		struct armv8_debugtui_pmu_result result = sentinel;
		bool uncertain;
		assert(transfer(&f, &result, &uncertain) == 0 && !uncertain && f.data_reads == 1);
		assert(result.value == (index == 0 ? f.proof[3] : index == 4 ? f.proof[5] : f.value));
		assert(result.pmcr == f.proof[3] && result.hdcr == f.proof[4] && result.pmselr == f.proof[5]);
		assert(result.dspsr == 0xa2000410 && result.dscr == 0x01000200);
		assert(f.gpr[0] == 0x11223344 && f.gpr[1] == 0x55667788);
		unsigned int operations = f.operations;
		for (unsigned int fail = 1; fail <= operations; fail++) {
			f = fresh(index); f.fail_at = fail; result = sentinel;
			assert(transfer(&f, &result, &uncertain) != 0 && uncertain);
			assert(f.operations == fail && memcmp(&result, &sentinel, sizeof(result)) == 0);
			failures++;
		}
		for (unsigned int el = 0; el < 2; el++)
			for (unsigned int hdd = 0; hdd < 2; hdd++) {
				f = fresh(index); f.dscr = (1u << 24) | (el << 8) | (hdd << 16); result = sentinel;
				assert(transfer(&f, &result, &uncertain) == DEBUGTUI_PMU_ACCESS_UNKNOWN && !uncertain);
				assert(f.injected == 0 && !f.data_reads && memcmp(&result, &sentinel, sizeof(result)) == 0);
				refused++;
			}
		for (unsigned int proof = 1; proof <= 6; proof++) {
			f = fresh(index); f.changed_proof = proof; result = sentinel;
			assert(transfer(&f, &result, &uncertain) != 0 && uncertain);
			assert(memcmp(&result, &sentinel, sizeof(result)) == 0); changed++;
		}
		for (unsigned int mismatch = 0; mismatch < 4; mismatch++) {
			f = fresh(index); result = sentinel;
			if (mismatch == 0) f.dscr_change = 1u << 16;
			else if (mismatch == 1) f.midr_change = 1u;
			else f.corrupt_gpr = mismatch - 1;
			/* R1 is used only by the genuine 64-bit cycle read. */
			if (mismatch == 3 && index != 22) continue;
			assert(transfer(&f, &result, &uncertain) != 0 && uncertain);
			assert(memcmp(&result, &sentinel, sizeof(result)) == 0); changed++;
		}
	}
	for (unsigned int count = 0; count <= 31; count++) {
		if (count == 4) continue;
		struct fixture f = fresh(17); f.proof[3] = 0x41130000 | (count << 11);
		struct armv8_debugtui_pmu_result result = sentinel; bool uncertain;
		assert(transfer(&f, &result, &uncertain) == DEBUGTUI_PMU_UNSUPPORTED && !uncertain && !f.data_reads);
		assert(memcmp(&result, &sentinel, sizeof(result)) == 0); refused++;
	}
	for (unsigned int variant = 0; variant < 12; variant++) {
		struct fixture f = fresh(22);
		int expected = DEBUGTUI_PMU_UNSUPPORTED;
		if (variant < 5) {
			const uint32_t states[] = {0x01000300, 0x01001200, 0x00000200, 0x01010200, 0x01000240};
			f.dscr = states[variant];
			if (variant == 3) expected = DEBUGTUI_PMU_RESTRICTED;
		} else if (variant == 5) f.midr = 0x411fd164;
		else if (variant < 10) f.proof[2] = (variant - 6) << 24; /* 0/1/2/3 */
		else if (variant == 10) f.proof[3] ^= 0x10000;
		else f.proof[5] = 32;
		if (variant == 9) f.proof[2] = 0x0f010066;
		struct armv8_debugtui_pmu_result result = sentinel; bool uncertain;
		assert(transfer(&f, &result, &uncertain) == (variant == 4 ? -1 : expected));
		assert(uncertain == (variant == 4) && !f.data_reads);
		assert(memcmp(&result, &sentinel, sizeof(result)) == 0); refused++;
	}
	for (unsigned int selected = 0; selected <= 31; selected++)
		for (unsigned int index = 7; index <= 8; index++) {
			struct fixture f = fresh(index); f.proof[5] = selected;
			struct armv8_debugtui_pmu_result result = sentinel; bool uncertain;
			bool valid = selected < 4 || (selected == 31 && index == 7);
			assert(transfer(&f, &result, &uncertain) == (valid ? 0 : DEBUGTUI_PMU_RESTRICTED) && !uncertain);
			assert(f.data_reads == (unsigned int)valid && f.proof[5] == selected);
			if (!valid) { assert(memcmp(&result, &sentinel, sizeof(result)) == 0); refused++; }
		}
	const uint64_t counters[] = {0, 0xffffffff, UINT64_C(0x100000000), UINT64_C(0x8000000000000000),
		UINT64_C(0xfffffffffffffffe), UINT64_MAX};
	for (unsigned int i = 0; i < sizeof(counters) / sizeof(counters[0]); i++) {
		struct fixture f = fresh(22); f.moving = true; f.value = counters[i];
		struct armv8_debugtui_pmu_result result; bool uncertain;
		assert(transfer(&f, &result, &uncertain) == 0 && !uncertain && result.value == counters[i]);
		assert(f.gpr[0] == 0x11223344 && f.gpr[1] == 0x55667788);
	}
	for (unsigned int control = 0; control < 4; control++) {
		struct fixture f = fresh(22); f.proof[3] = 0x41132000 | (control & 1) | ((control & 2) << 5);
		struct armv8_debugtui_pmu_result result; bool uncertain;
		assert(transfer(&f, &result, &uncertain) == 0 && !uncertain);
		assert(result.value == UINT64_C(0xfedcba9876543210) && result.pmcr == f.proof[3]);
	}
	printf("PASS: 23 native PMU encodings, %u transport failures, %u safe refusals, %u proof/scratch changes, 6 coherent moving cycle reads; no control or selector writes\n", failures, refused, changed);
	return 0;
}
