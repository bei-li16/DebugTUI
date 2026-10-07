/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Exercise the production header; the fixture models transport only. */
#include <assert.h>
#include <stdio.h>
#include "armv8_debugtui_gic.h"

/* Independent AArch32 MRC R0 transcriptions from R52 Tables 10-69/80. */
static const uint32_t opcodes[] = {
	0xee1c0f9c, 0xee1c0fbc, 0xee9c0fb9, 0xee140f16, 0xee1c0f7b,
	0xee1c0f78, 0xee1c0f7c, 0xee1c0fdc, 0xee1c0ffc, 0xee1c0f58, 0xee1c0f5c,
	0xee1c0f98, 0xee1c0fb8, 0xee1c0fd8, 0xee1c0ff8,
	0xee1c0f19, 0xee1c0f39, 0xee1c0f59, 0xee1c0f79,
	0xee9c0f3b, 0xee9c0f1b, 0xee9c0f5b, 0xee9c0f7b, 0xee9c0fbb, 0xee9c0ffb,
	0xee9c0f18, 0xee9c0f38, 0xee9c0f58, 0xee9c0f78,
	0xee9c0f19, 0xee9c0f39, 0xee9c0f59, 0xee9c0f79,
	0xee9c0f1c, 0xee9c0f3c, 0xee9c0f5c, 0xee9c0f7c,
	0xee9c0f1e, 0xee9c0f3e, 0xee9c0f5e, 0xee9c0f7e,
	0xee1c0f18, 0xee1c0f1c,
};
static const uint32_t proof_opcodes[] = {
	0xee740f15, 0xee740f35, 0xee100f31, 0xee9c0fb9, 0xee1c0fbc,
	0xee1c0f9c, 0xee9c0f3b, 0xee910f11, 0xee9c0f1b, 0xee910f71,
};
struct fixture {
	uint32_t gpr, midr, dscr, proof[10], value;
	unsigned int index, operations, fail_at, injected, data_reads, debug_reads;
	unsigned int changed_proof, dscr_change_at;
	uint32_t dscr_change, midr_change;
	bool corrupt_scratch;
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
	*value = offset == 0x088 ? f->dscr ^ (f->debug_reads == f->dscr_change_at ? f->dscr_change : 0) :
		f->midr ^ (f->debug_reads == 4 ? f->midr_change : 0);
	return 0;
}
static int read_gpr(void *context, unsigned int reg, uint32_t *value)
{
	struct fixture *f = context;
	assert(reg == 0);
	if (step(f))
		return -7;
	*value = f->gpr;
	return 0;
}
static int write_gpr(void *context, unsigned int reg, uint32_t value)
{
	struct fixture *f = context;
	assert(reg == 0);
	if (step(f))
		return -7;
	f->gpr = value ^ (unsigned int)f->corrupt_scratch;
	return 0;
}
static uint32_t data_value(const struct fixture *f)
{
	switch (f->index) {
	case 0: return f->proof[5];
	case 1: return f->proof[4];
	case 2: return f->proof[3];
	case 19: return f->proof[6];
	case 20: return f->proof[8];
	default: return f->value;
	}
}
static int execute(void *context, uint32_t opcode)
{
	struct fixture *f = context;
	if (step(f))
		return -7;
	if (f->injected == 10) {
		assert(f->index < 41 && opcode == opcodes[f->index]);
		f->data_reads++;
		f->gpr = data_value(f);
	} else {
		unsigned int index = f->injected < 10 ? f->injected : f->injected - 11;
		assert(index < 10 && opcode == proof_opcodes[index]);
		f->gpr = f->proof[index] ^ (f->data_reads && f->changed_proof == index + 1 ? 1 : 0);
	}
	f->injected++;
	return 0;
}
static struct fixture fresh(unsigned int index)
{
	return (struct fixture){.gpr = 0x11223344, .midr = 0x411fd134,
		.dscr = 0x01050213, .proof = {0xa2000410, 0x81234568, 0x10111011,
			15, 7, 0x403, 0x90180003, 0x38, 0x7c01, 0x1000},
		.index = index, .value = 0xf1234500 + index};
}
static int transfer(struct fixture *f, struct armv8_debugtui_gic_result *result, bool *uncertain)
{
	struct armv8_debugtui_timer_io io = {
		.core = {.context = f, .read_gpr = read_gpr, .write_gpr = write_gpr, .execute = execute},
		.read_debug = read_debug,
	};
	return armv8_debugtui_gic_transfer(&io, &armv8_debugtui_gic_registers[f->index], result, uncertain);
}
static bool implemented(unsigned int index)
{
	return !(index >= 12 && index <= 14) && !(index >= 16 && index <= 18) &&
		!(index >= 26 && index <= 28) && !(index >= 30 && index <= 32) && index < 41;
}
int main(void)
{
	assert(sizeof(opcodes) / sizeof(opcodes[0]) == 43);
	assert(sizeof(armv8_debugtui_gic_registers) / sizeof(armv8_debugtui_gic_registers[0]) == 43);
	assert(strcmp(armv8_debugtui_gic_reason(1), "debugtui-gic:reader-unsupported") == 0);
	assert(strcmp(armv8_debugtui_gic_reason(2), "debugtui-gic:access-restricted") == 0);
	assert(strcmp(armv8_debugtui_gic_reason(3), "debugtui-gic:access-unknown") == 0);
	assert(strcmp(armv8_debugtui_gic_reason(4), "debugtui-gic:not-implemented") == 0);
	for (unsigned int i = 0; i < 43; i++)
		assert(armv8_debugtui_gic_find(armv8_debugtui_gic_registers[i].name) == &armv8_debugtui_gic_registers[i]);
	assert(!armv8_debugtui_gic_find("icc_eoir0") && !armv8_debugtui_gic_find("icc_dir") &&
		!armv8_debugtui_gic_find("icc_sgi1r") && !armv8_debugtui_gic_find("icv_ap0r0") &&
		!armv8_debugtui_gic_find("ich_lr4"));
	unsigned int failures = 0, refused = 0, changed = 0, valid = 0;
	struct armv8_debugtui_gic_result sentinel;
	memset(&sentinel, 0xa5, sizeof(sentinel));
	for (unsigned int index = 0; index < 43; index++) {
		struct fixture f = fresh(index);
		struct armv8_debugtui_gic_result result = sentinel;
		bool uncertain;
		int expected = index >= 41 ? DEBUGTUI_GIC_RESTRICTED :
			implemented(index) ? 0 : DEBUGTUI_GIC_NOT_IMPLEMENTED;
		assert(transfer(&f, &result, &uncertain) == expected && !uncertain);
		assert(f.data_reads == (unsigned int)implemented(index));
		if (!implemented(index)) {
			assert(memcmp(&result, &sentinel, sizeof(result)) == 0);
			assert(f.injected == (index < 41 ? 10u : 0u));
			refused++;
		} else {
			valid++;
			assert(result.value == data_value(&f) && result.icc_ctlr == 0x403 && result.ich_vtr == 0x90180003);
			assert(result.hcr == 0x38 && result.ich_hcr == 0x7c01 && result.hstr == 0x1000);
			assert(result.dspsr == 0xa2000410 && f.gpr == 0x11223344);
			unsigned int operations = f.operations;
			for (unsigned int fail = 1; fail <= operations; fail++) {
				f = fresh(index); f.fail_at = fail; result = sentinel;
				assert(transfer(&f, &result, &uncertain) != 0 && uncertain && f.operations == fail);
				assert(memcmp(&result, &sentinel, sizeof(result)) == 0); failures++;
			}
			for (unsigned int proof = 1; proof <= 10; proof++) {
				f = fresh(index); f.changed_proof = proof; result = sentinel;
				assert(transfer(&f, &result, &uncertain) != 0 && uncertain);
				assert(memcmp(&result, &sentinel, sizeof(result)) == 0); changed++;
			}
			for (unsigned int mismatch = 0; mismatch < 4; mismatch++) {
				f = fresh(index); result = sentinel;
				if (mismatch < 2) { f.dscr_change_at = mismatch == 0 ? 3 : 5; f.dscr_change = 1u << 8; }
				else if (mismatch == 2) f.midr_change = 1u;
				else f.corrupt_scratch = true;
				assert(transfer(&f, &result, &uncertain) != 0 && uncertain);
				assert(memcmp(&result, &sentinel, sizeof(result)) == 0); changed++;
			}
		}
		if (index >= 41) continue;
		for (unsigned int el = 0; el < 2; el++)
			for (unsigned int hdd = 0; hdd < 2; hdd++) {
				f = fresh(index); f.dscr = (1u << 24) | UINT32_C(0x00050013) | (el << 8) | (hdd << 15); result = sentinel;
				assert(transfer(&f, &result, &uncertain) == DEBUGTUI_GIC_ACCESS_UNKNOWN && !uncertain);
				assert(!f.injected && !f.data_reads && memcmp(&result, &sentinel, sizeof(result)) == 0); refused++;
			}
	}
	for (unsigned int word = 0; word < 4; word++)
		for (unsigned int bits = 0; bits < (word == 3 ? 32u : 8u); bits++) {
			if (bits == (word == 3 ? 3u : 4u)) continue;
			struct fixture f = fresh(11); bool uncertain;
			unsigned int expected_proof;
			if (word == 0) { f.proof[5] = bits << 8; expected_proof = 6; }
			else { f.proof[6] = (f.proof[6] & ~(word == 3 ? 31u : 7u << (word == 1 ? 29 : 26))) |
				(bits << (word == 1 ? 29 : word == 2 ? 26 : 0)); expected_proof = 7; }
			struct armv8_debugtui_gic_result result = sentinel;
			assert(transfer(&f, &result, &uncertain) == DEBUGTUI_GIC_UNSUPPORTED && !uncertain);
			assert(f.injected == expected_proof && !f.data_reads && memcmp(&result, &sentinel, sizeof(result)) == 0); refused++;
		}
	for (unsigned int variant = 0; variant < 9; variant++) {
		struct fixture f = fresh(25); bool uncertain;
		if (variant == 0) f.midr = 0x411fd164;
		else if (variant == 1) f.dscr = 0x01001200;
		else if (variant == 2) f.dscr = 0x00000200;
		else if (variant == 3) f.dscr = 0x01008200;
		else if (variant == 4) f.dscr = 0x01000240;
		else if (variant == 5) f.proof[2] = 0;
		else if (variant == 6) f.proof[3] = 7;
		else if (variant == 7) f.proof[4] = 0;
		else f.proof[6] ^= 1u << 5;
		struct armv8_debugtui_gic_result result = sentinel;
		assert(transfer(&f, &result, &uncertain) == (variant == 3 ? DEBUGTUI_GIC_RESTRICTED : variant == 4 ? -1 : DEBUGTUI_GIC_UNSUPPORTED));
		assert(uncertain == (variant == 4) && !f.data_reads && memcmp(&result, &sentinel, sizeof(result)) == 0); refused++;
	}
	assert(valid == 29);
	printf("PASS: 43 fixed GIC routes, 29 observational reads, %u transport failures, %u safe refusals, %u proof/scratch changes; no acknowledge, enable or control writes\n", failures, refused, changed);
	return 0;
}
