/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Compile the exact production bank transaction; no target/probe access. */
#include <assert.h>
#include <stdio.h>
#include "armv8_debugtui_banked.h"

struct fixture {
	uint32_t gpr[16], cpsr, midr, bank_value, last_opcode;
	unsigned int operations, fail_at, restores, cpsr_reads;
	bool corrupt_restore;
	uint32_t change_status_bits;
};

static int read_gpr(void *context, unsigned int reg, uint32_t *value)
{
	struct fixture *f = context;
	assert(reg == 0);
	if (++f->operations == f->fail_at)
		return -7;
	*value = f->gpr[reg];
	return 0;
}

static int write_gpr(void *context, unsigned int reg, uint32_t value)
{
	struct fixture *f = context;
	assert(reg == 0);
	if (++f->operations == f->fail_at)
		return -7;
	f->restores++;
	f->gpr[reg] = value ^ (f->corrupt_restore ? 1 : 0);
	return 0;
}

static int execute(void *context, uint32_t opcode)
{
	struct fixture *f = context;
	if (++f->operations == f->fail_at)
		return -7;
	f->last_opcode = opcode;
	if (opcode == DEBUGTUI_READ_DSPSR) {
		f->cpsr_reads++;
		f->gpr[0] = f->cpsr ^ (f->cpsr_reads == 2 ? f->change_status_bits : 0);
	} else if (opcode == ARMV4_5_MRC(15, 0, 0, 0, 0, 0)) {
		f->gpr[0] = f->midr;
	} else if ((opcode & UINT32_C(0xfffffff0)) == UINT32_C(0xea4f0000)) {
		f->gpr[0] = f->gpr[opcode & 15];
	} else {
		/* Only ordinary SPSR or banked MRS is admitted. No MSR/CPS/VMSR. */
		assert(opcode == DEBUGTUI_MRS_SPSR ||
			(opcode & UINT32_C(0xffe0ffe0)) == UINT32_C(0xf3e08020));
		f->gpr[0] = f->bank_value;
	}
	return 0;
}

static struct armv8_debugtui_io io_for(struct fixture *fixture)
{
	struct armv8_debugtui_io io = {
		.context = fixture, .read_gpr = read_gpr, .write_gpr = write_gpr, .execute = execute,
	};
	return io;
}

int main(void)
{
	const struct armv8_debugtui_bank *sp_irq = armv8_debugtui_bank_find("sp_irq");
	assert(sp_irq);
	assert(!armv8_debugtui_bank_find("sp_mon"));
	assert(!armv8_debugtui_bank_find("spsr_usr"));
	assert(DEBUGTUI_READ_DSPSR == UINT32_C(0xee740f15)); /* Independent GNU assembly. */
	for (unsigned int failure = 0; failure <= 20; failure++) {
		struct fixture f = {.gpr = {0x11223344}, .cpsr = 0xa000001a, .midr = 0x411fd134,
			.bank_value = 0x88776655, .fail_at = failure};
		struct armv8_debugtui_io io = io_for(&f);
		uint32_t value = 0xdeadbeef;
		bool uncertain = false;
		int result = armv8_debugtui_bank_transfer(&io, sp_irq, &value, &uncertain);
		if (failure) {
			assert(result == -7 && uncertain && f.operations == failure);
			assert(value == 0xdeadbeef); /* Never return partial data or inject rollback. */
		} else {
			assert(result == 0 && !uncertain && value == 0x88776655);
			assert(f.operations == 20 && f.restores == 4 && f.gpr[0] == 0x11223344);
		}
	}
	const uint32_t changed_bits[] = {0, 1, 1u << 5, 1u << 10, 1u << 26, 1u << 31};
	for (unsigned int fault = 0; fault < sizeof(changed_bits) / sizeof(changed_bits[0]); fault++) {
		struct fixture f = {.gpr = {42}, .cpsr = 0x1a, .midr = 0x411fd134,
			.corrupt_restore = fault == 0, .change_status_bits = changed_bits[fault]};
		struct armv8_debugtui_io io = io_for(&f);
		uint32_t value = 99;
		bool uncertain = false;
		assert(armv8_debugtui_bank_transfer(&io, sp_irq, &value, &uncertain) != 0);
		assert(uncertain && value == 99);
	}
	for (unsigned int refused = 0; refused < 3; refused++) {
		struct fixture f = {.gpr = {42}, .cpsr = refused == 0 ? 0x10 : 0x13,
			.midr = refused == 1 ? 0x411fc153 : 0x411fd134};
		struct armv8_debugtui_io io = io_for(&f);
		uint32_t value = 99;
		bool uncertain = true;
		const struct armv8_debugtui_bank *bank = refused == 2 ? armv8_debugtui_bank_find("sp_hyp") : sp_irq;
		int result = armv8_debugtui_bank_transfer(&io, bank, &value, &uncertain);
		assert(result == (refused == 1 ? DEBUGTUI_BANKED_UNSUPPORTED : DEBUGTUI_BANKED_UNAVAILABLE));
		assert(!uncertain && value == 99 && f.gpr[0] == 42);
		assert(f.operations == (refused == 1 ? 10 : 5));
	}
	/* Independent GNU Arm assembly words and architecture's current-bank rules. */
	const struct { const char *name; unsigned int mode; uint32_t opcode; } cases[] = {
		{"sp_irq", 0x1a, 0xf3e18030}, {"sp_irq", 0x12, 0xea4f000d},
		{"spsr_irq", 0x1a, 0xf3f08030}, {"spsr_irq", 0x12, 0xf3ff8000},
		{"r8_fiq", 0x13, 0xf3e88020}, {"r8_fiq", 0x11, 0xea4f0008},
		{"r8_usr", 0x11, 0xf3e08020}, {"r8_usr", 0x13, 0xea4f0008},
		{"sp_usr", 0x1f, 0xea4f000d}, {"lr_usr", 0x1a, 0xea4f000e},
		{"sp_hyp", 0x1a, 0xea4f000d}, {"spsr_hyp", 0x1a, 0xf3ff8000},
		{"elr_hyp", 0x1a, 0xf3ee8030},
	};
	for (unsigned int i = 0; i < sizeof(cases) / sizeof(cases[0]); i++) {
		uint32_t opcode = 0;
		assert(armv8_debugtui_bank_opcode(armv8_debugtui_bank_find(cases[i].name), cases[i].mode, &opcode) == 0);
		assert(opcode == cases[i].opcode);
	}
	for (unsigned int i = 0; i < sizeof(armv8_debugtui_banks) / sizeof(armv8_debugtui_banks[0]); i++) {
		const struct armv8_debugtui_bank *bank = &armv8_debugtui_banks[i];
		uint32_t opcode;
		assert(armv8_debugtui_bank_opcode(bank, 0x16, &opcode) == DEBUGTUI_BANKED_UNSUPPORTED);
		if (bank->mode == 0x1a)
			assert(armv8_debugtui_bank_opcode(bank, 0x13, &opcode) == DEBUGTUI_BANKED_UNAVAILABLE);
	}
	puts("PASS: physical bank read, 20 failure points, restoration/CPSR mismatches, safe refusals and GNU encodings");
	return 0;
}
