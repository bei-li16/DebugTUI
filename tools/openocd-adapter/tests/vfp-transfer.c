/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Compile the production transaction. Only its physical I/O is modeled. */
#include <assert.h>
#include <stdio.h>
#include "armv8_debugtui_vfp.h"

struct fixture {
	uint32_t gpr[2], dspsr, midr, hcptr, mvfr0, mvfr1, fpexc;
	unsigned int operations, fail_at, status_reads, enable_reads, trap_reads, data_reads;
	unsigned int corrupt_restore;
	uint32_t status_change, enable_change, trap_change;
};
static int read_gpr(void *context, unsigned int reg, uint32_t *value)
{
	struct fixture *f = context;
	assert(reg < 2);
	if (++f->operations == f->fail_at)
		return -7;
	*value = f->gpr[reg];
	return 0;
}
static int write_gpr(void *context, unsigned int reg, uint32_t value)
{
	struct fixture *f = context;
	assert(reg < 2);
	if (++f->operations == f->fail_at)
		return -7;
	f->gpr[reg] = value ^ (f->corrupt_restore == reg + 1 ? 1 : 0);
	return 0;
}
static uint64_t data(unsigned int index)
{
	return UINT64_C(0xfedcba9876543200) + index;
}
static int execute(void *context, uint32_t opcode)
{
	struct fixture *f = context;
	if (++f->operations == f->fail_at)
		return -7;
	if (opcode == DEBUGTUI_READ_DSPSR)
		f->gpr[0] = f->dspsr ^ (++f->status_reads == 2 ? f->status_change : 0);
	else if (opcode == ARMV4_5_MRC(15, 0, 0, 0, 0, 0))
		f->gpr[0] = f->midr;
	else if (opcode == DEBUGTUI_READ_HCPTR)
		f->gpr[0] = f->hcptr ^ (++f->trap_reads == 2 ? f->trap_change : 0);
	else if ((opcode & UINT32_C(0xfff00fff)) == DEBUGTUI_VMRS(0)) {
		switch ((opcode >> 16) & 15) {
		case 0: f->gpr[0] = 0x41034025; break;
		case 1: f->gpr[0] = 0xa000009f; break;
		case 5: f->gpr[0] = 0x43; break;
		case 6: f->gpr[0] = f->mvfr1; break;
		case 7: f->gpr[0] = f->mvfr0; break;
		case 8: f->gpr[0] = f->fpexc ^ (++f->enable_reads == 2 ? f->enable_change : 0); break;
		default: assert(false);
		}
	} else {
		assert((opcode & UINT32_C(0xfff00fd0)) == UINT32_C(0xec500b10));
		unsigned int index = (opcode & 15) | ((opcode & 32) >> 1);
		uint64_t value = data(index);
		f->gpr[0] = (uint32_t)value;
		f->gpr[1] = value >> 32;
		f->data_reads++;
	}
	return 0;
}
static struct fixture fresh(void)
{
	struct fixture f = {.gpr = {0x11223344, 0x55667788}, .dspsr = 0xa200041a,
		.midr = 0x411fd134, .mvfr0 = 0x10110222, .mvfr1 = 0x12111111, .fpexc = 0x40000700};
	return f;
}
static int transfer(struct fixture *fixture, const char *name,
	struct armv8_debugtui_vfp_result *result, bool *uncertain)
{
	struct armv8_debugtui_vfp_request request;
	assert(armv8_debugtui_vfp_request(name, &request));
	struct armv8_debugtui_io io = {.context = fixture, .read_gpr = read_gpr,
		.write_gpr = write_gpr, .execute = execute};
	return armv8_debugtui_vfp_transfer(&io, &request, result, uncertain);
}
int main(void)
{
	struct armv8_debugtui_vfp_request request;
	const char *invalid[] = {"d32", "q16", "d01", "D0", "s0", "fpinst", "d-1", "d0; reset", ""};
	for (unsigned int i = 0; i < sizeof(invalid) / sizeof(invalid[0]); i++)
		assert(!armv8_debugtui_vfp_request(invalid[i], &request));
	/* GNU Arm assembler independently produces these words. */
	assert(DEBUGTUI_VMRS(1) == 0xeef10a10 && DEBUGTUI_VMRS(7) == 0xeef70a10);
	assert(ARMV5_T_MRRC(11, 1, 0, 1, 0) == 0xec510b10);
	assert(ARMV5_T_MRRC(11, 3, 0, 1, 15) == 0xec510b3f);
	for (unsigned int failure = 0; failure <= 63; failure++) {
		struct fixture f = fresh(); f.fail_at = failure;
		struct armv8_debugtui_vfp_result result = {.words = {11, 22}};
		bool uncertain = false;
		int status = transfer(&f, "q15", &result, &uncertain);
		if (failure) {
			assert(status == -7 && uncertain && f.operations == failure);
			assert(result.words[0] == 11 && result.words[1] == 22);
		} else {
			assert(status == 0 && !uncertain && f.operations == 63 && f.data_reads == 2);
			assert(result.words[0] == data(30) && result.words[1] == data(31));
			assert(f.gpr[0] == 0x11223344 && f.gpr[1] == 0x55667788);
			assert(result.fpexc == 0x40000700 && result.neon);
		}
	}
	for (unsigned int changed = 0; changed < 6; changed++) {
		struct fixture f = fresh();
		if (changed < 2) f.corrupt_restore = changed + 1;
		if (changed == 2) f.status_change = 1u << 5;
		if (changed == 3) f.status_change = 1u << 26;
		if (changed == 4) f.enable_change = 1u << 30;
		if (changed == 5) f.trap_change = 1u << 10;
		struct armv8_debugtui_vfp_result result = {.words = {11, 22}};
		bool uncertain = false;
		assert(transfer(&f, "d0", &result, &uncertain) != 0 && uncertain);
		assert(result.words[0] == 11 && result.words[1] == 22);
	}
	for (unsigned int refused = 0; refused < 7; refused++) {
		struct fixture f = fresh();
		const char *name = "d16";
		int expected = DEBUGTUI_VFP_RESTRICTED;
		unsigned int steps = 5;
		if (refused < 2) f.dspsr = refused == 0 ? 0x10 : 0x13;
		if (refused == 2) { f.hcptr = 1u << 10; steps = 15; }
		if (refused == 3) { f.fpexc = 0x700; steps = 30; expected = DEBUGTUI_VFP_DISABLED; }
		if (refused == 4 || refused == 5) {
			f.mvfr0 = 0x10110021; f.mvfr1 = 0x11000011; steps = 30;
			expected = DEBUGTUI_VFP_NOT_IMPLEMENTED; if (refused == 5) name = "q0";
		}
		if (refused == 6) { f.mvfr1 |= 2u << 12; steps = 25; expected = DEBUGTUI_VFP_UNSUPPORTED; }
		struct armv8_debugtui_vfp_result result = {.words = {11, 22}};
		bool uncertain = true;
		assert(transfer(&f, name, &result, &uncertain) == expected && !uncertain);
		assert(f.operations == steps && f.data_reads == 0);
		assert(f.gpr[0] == 0x11223344 && f.gpr[1] == 0x55667788);
	}
	for (unsigned int disabled = 0; disabled < 2; disabled++) {
		struct fixture f = fresh(); f.mvfr0 = 0x10110021; f.mvfr1 = 0x11000011;
		if (disabled) f.fpexc &= ~(1u << 30);
		struct armv8_debugtui_vfp_result result;
		bool uncertain = true;
		assert(transfer(&f, disabled ? "mvfr0" : "d15", &result, &uncertain) == 0 && !uncertain);
		assert(!result.neon && result.mvfr0 == 0x10110021);
		assert(!disabled || result.words[0] == 0x10110021);
		if (!disabled) assert(result.words[0] == data(14) && result.words[1] == data(15));
	}
	/* Preserve raw unknown feature encodings for inspection, never authorize data. */
	struct fixture unknown = fresh(); unknown.mvfr1 = 0x12113111;
	struct armv8_debugtui_vfp_result raw;
	bool uncertain = true;
	assert(transfer(&unknown, "mvfr1", &raw, &uncertain) == 0 && !uncertain);
	assert(raw.words[0] == 0x12113111 && raw.mvfr1 == 0x12113111 && unknown.data_reads == 0);
	puts("PASS: VFP pairs, 63 fault points, physical scratch/control restoration, D16/D32, disabled and safe refusals");
	return 0;
}
