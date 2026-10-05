/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Compile the exact production transaction; independent GNU opcode witnesses. */
#include <assert.h>
#include <stdio.h>
#include "armv8_debugtui_banked.h"
struct fixture {
	uint32_t gpr[16], dspsr, dlr, cpsr, midr, dscr, bank_value;
	unsigned int operations, fail_at, restores, cpsr_reads, dspsr_reads, dlr_reads, data_reads;
	bool corrupt_restore;
	uint32_t change_cpsr, change_dspsr, change_dlr, change_midr, change_dscr;
};
static int next(struct fixture *f) { return ++f->operations == f->fail_at ? -7 : 0; }
static int read_gpr(void *context, unsigned int reg, uint32_t *value)
{
	struct fixture *f = context; assert(reg == 0);
	if (next(f)) return -7;
	*value = f->gpr[reg]; return 0;
}
static int write_gpr(void *context, unsigned int reg, uint32_t value)
{
	struct fixture *f = context; assert(reg == 0);
	if (next(f)) return -7;
	f->restores++; f->gpr[reg] = value ^ (f->corrupt_restore ? 1 : 0); return 0;
}
static int read_debug(void *context, uint32_t offset, uint32_t *value)
{
	struct fixture *f = context; if (next(f)) return -7;
	if (offset == 0x88) *value = f->dscr ^ (f->operations > 2 ? f->change_dscr : 0);
	else { assert(offset == 0xd00); *value = f->midr ^ (f->operations > 2 ? f->change_midr : 0); }
	return 0;
}
static int execute(void *context, uint32_t opcode)
{
	struct fixture *f = context; if (next(f)) return -7;
	if (opcode == 0xee740f15) f->gpr[0] = f->dspsr ^ (++f->dspsr_reads == 2 ? f->change_dspsr : 0);
	else if (opcode == 0xee740f35) f->gpr[0] = f->dlr ^ (++f->dlr_reads == 2 ? f->change_dlr : 0);
	else if ((opcode & UINT32_C(0xfffffff0)) == UINT32_C(0xea4f0000)) {
		f->data_reads++; f->gpr[0] = f->gpr[opcode & 15];
	} else {
		/* Never permit CPU MIDR MRC, MSR/CPS, DCPS or control writes. */
		assert(opcode == 0xf3ff8000 || (opcode & UINT32_C(0xffe0ffe0)) == UINT32_C(0xf3e08020));
		f->data_reads++; f->gpr[0] = f->bank_value;
	}
	return 0;
}
static struct armv8_debugtui_banked_io io_for(struct fixture *f)
{
	struct armv8_debugtui_banked_io io = {
		.core = {.context = f, .read_gpr = read_gpr, .write_gpr = write_gpr, .execute = execute}, .read_debug = read_debug,
	}; return io;
}
static struct fixture fixture(unsigned int mode)
{
	struct fixture f = {.dspsr = 0xa2000410, .dlr = 0x81234568, .cpsr = 0xa00f0200 | mode,
		.midr = 0x411fd134, .dscr = 0x01000000 | ((mode == 0x10 ? 0 : mode == 0x1a ? 2 : 1) << 8), .bank_value = 0x88776655};
	for (unsigned int i = 0; i < 16; i++) f.gpr[i] = 0x11223300 + i;
	return f;
}
int main(void)
{
	const struct armv8_debugtui_bank *irq = armv8_debugtui_bank_find("sp_irq");
	assert(irq && !armv8_debugtui_bank_find("sp_mon") && !armv8_debugtui_bank_find("spsr_usr"));
	assert(DEBUGTUI_READ_DSPSR == 0xee740f15 && DEBUGTUI_BANKED_READ_DLR == 0xee740f35);
	/* All thirty hand-assembled GNU words, in the documented name order. */
	const uint32_t banked[] = {0xf3e08020,0xf3e18020,0xf3e28020,0xf3e38020,0xf3e48020,
		0xf3e58020,0xf3e68020,0xf3e88020,0xf3e98020,0xf3ea8020,0xf3eb8020,0xf3ec8020,
		0xf3ed8020,0xf3ee8020,0xf3fe8020,0xf3e08030,0xf3e18030,0xf3f08030,
		0xf3e28030,0xf3e38030,0xf3f28030,0xf3e48030,0xf3e58030,0xf3f48030,
		0xf3e68030,0xf3e78030,0xf3f68030,0xf3ee8030,0xf3ef8030,0xf3fe8030};
	const unsigned int modes[] = {0x10,0x11,0x12,0x13,0x17,0x1a,0x1b,0x1f};
	unsigned int accepted = 0, refused = 0, unknown = 0, failures = 0;
	for (unsigned int m = 0; m < 8; m++) for (unsigned int i = 0; i < 30; i++) {
		unsigned int mode = modes[m]; const struct armv8_debugtui_bank *bank = &armv8_debugtui_banks[i];
		bool current = (i < 5 && mode != 0x11) || (i == 5 && (mode == 0x10 || mode == 0x1f)) ||
			(i == 6 && (mode == 0x10 || mode == 0x1f || mode == 0x1a)) || (i >= 7 && i != 27 && bank->mode == mode);
		bool legal_opcode = current || (mode != 0x10 && (i < 27 || mode == 0x1a));
		bool permitted = mode == 0x1a || (mode == 0x10 && current);
		uint32_t opcode = 0xdeadbeef; int code = armv8_debugtui_bank_opcode(bank, mode, &opcode);
		if (!legal_opcode) assert(code == DEBUGTUI_BANKED_UNAVAILABLE && opcode == 0xdeadbeef);
		else assert(code == 0 && opcode == (current ? (bank->spsr ? 0xf3ff8000 : 0xea4f0000 | bank->gpr) : banked[i]));
		for (unsigned int fail = 0; fail <= (permitted ? 30 : 0); fail++) {
			struct fixture f = fixture(mode);
			if (mode == 0x10) { f.cpsr = 0xa00f021a; f.change_cpsr = 31; } /* UNKNOWN mode bits. */
			f.fail_at = fail; struct armv8_debugtui_banked_io io = io_for(&f);
			struct armv8_debugtui_banked_result result = {.value = 0xdeadbeef}; bool uncertain = true;
			code = armv8_debugtui_bank_transfer(&io, bank, &result, &uncertain);
			if (fail) {
				assert(code == -7 && uncertain && f.operations == fail && result.value == 0xdeadbeef); failures++;
			} else if (!permitted) {
				assert(code == (mode != 0x10 && i < 27 ? DEBUGTUI_BANKED_ACCESS_UNKNOWN : DEBUGTUI_BANKED_UNAVAILABLE));
				assert(!uncertain && f.data_reads == 0 && f.cpsr_reads == 0 && result.value == 0xdeadbeef);
				if (code == DEBUGTUI_BANKED_ACCESS_UNKNOWN) unknown++; else refused++;
			} else {
				assert(code == 0 && !uncertain && f.operations == 30 && f.restores == 5 && f.data_reads == 1);
				assert(result.value == (current && !bank->spsr ? 0x11223300 + bank->gpr : 0x88776655));
				assert(result.dspsr == 0xa2000410 && result.midr == f.midr && result.dlr == f.dlr);
				assert(strcmp(result.method, current ? (bank->spsr ? "mrs32" : "mov32") : "banked_mrs32") == 0);
				assert(f.gpr[0] == 0x11223300); accepted++;
			}
		}
	}
	for (unsigned int mutation = 0; mutation < 5; mutation++) {
		struct fixture f = fixture(0x1a);
		if (mutation == 0) f.corrupt_restore = true;
		if (mutation == 1) f.change_dspsr = 1u << 26;
		if (mutation == 2) f.change_dlr = 4;

		if (mutation == 3) f.change_midr = 1;
		if (mutation == 4) f.change_dscr = 1u << 8;
		struct armv8_debugtui_banked_io io = io_for(&f); struct armv8_debugtui_banked_result result = {.value = 99}; bool uncertain;
		assert(armv8_debugtui_bank_transfer(&io, irq, &result, &uncertain) != 0 && uncertain && result.value == 99);
	}
	for (unsigned int mutation = 0; mutation < 7; mutation++) {
		struct fixture f = fixture(0x1a);
		if (mutation == 0) f.midr = 0x411fc153;
		if (mutation == 1) f.midr = 0x511fd134;
		if (mutation == 2) f.midr = 0x411ed134;
		if (mutation == 3) f.dscr |= 1u << 16;
		if (mutation == 4) f.dscr |= 1u << 12;
		if (mutation == 5) f.dscr &= ~(1u << 24);
		if (mutation == 6) f.dscr |= 1u << 6;
		struct armv8_debugtui_banked_io io = io_for(&f); struct armv8_debugtui_banked_result result = {.value = 99}; bool uncertain;
		int code = armv8_debugtui_bank_transfer(&io, irq, &result, &uncertain);
		assert(code != 0 && f.data_reads == 0 && result.value == 99 && f.operations <= 2 && uncertain == (mutation == 6));
	}
	printf("PASS: %u EL0/EL2 bank reads, %u safe restrictions, %u EL1 unknown refusals, %u failure points; external identity, stopped DSPSR/DLR and scratch proof; no current CPSR read\n", accepted, refused, unknown, failures);
	return 0;
}
