/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Execute the production transaction against independent physical bit storage. */
#include <assert.h>
#include <stdio.h>
#include "armv8_debugtui_vfp_write.h"

struct fixture {
	uint32_t gpr[2], dspsr, midr, hcptr, mvfr0, mvfr1, fpexc;
	uint64_t d[32];
	unsigned int operations, fail_at, writes, corrupt_restore;
	bool fail_after_commit;
	unsigned int changed, corrupt_data;
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
	bool corrupt = f->writes && f->corrupt_restore == reg + 1 &&
		value == (reg ? UINT32_C(0x55667788) : UINT32_C(0x11223344));
	f->gpr[reg] = value ^ (corrupt ? 1 : 0);
	return 0;
}

static int execute(void *context, uint32_t opcode)
{
	struct fixture *f = context;
	bool write = (opcode & UINT32_C(0xfff00fd0)) == UINT32_C(0xec400b10);
	if (++f->operations == f->fail_at && !(write && f->fail_after_commit))
		return -7;
	if (opcode == DEBUGTUI_READ_DSPSR)
		f->gpr[0] = f->dspsr ^ (f->writes && f->changed == 1 ? 1u << 26 : 0);
	else if (opcode == ARMV4_5_MRC(15, 0, 0, 0, 0, 0))
		f->gpr[0] = f->midr;
	else if (opcode == DEBUGTUI_READ_HCPTR)
		f->gpr[0] = f->hcptr ^ (f->writes && f->changed == 2 ? 1u << 15 : 0);
	else if ((opcode & UINT32_C(0xfff00fff)) == DEBUGTUI_VMRS(0)) {
		switch ((opcode >> 16) & 15) {
		case 7: f->gpr[0] = f->mvfr0 ^ (f->writes && f->changed == 3 ? 1u << 28 : 0); break;
		case 6: f->gpr[0] = f->mvfr1 ^ (f->writes && f->changed == 4 ? 1u << 28 : 0); break;
		case 8: f->gpr[0] = f->fpexc ^ (f->writes && f->changed == 5 ? 1u << 30 : 0); break;
		default: assert(false); /* No control writes or unrelated instructions. */
		}
	} else {
		unsigned int index = (opcode & 15) | ((opcode & 32) >> 1);
		assert(index < 32);
		if (write) {
			f->d[index] = ((uint64_t)f->gpr[1] << 32) | f->gpr[0];
			f->writes++;
			if (f->operations == f->fail_at)
				return -7; /* The failed execute actually changed physical data. */
		} else {
			assert((opcode & UINT32_C(0xfff00fd0)) == UINT32_C(0xec500b10));
			uint64_t value = f->d[index];
			if (f->writes && f->corrupt_data == index + 1)
				value ^= UINT64_C(0x100000000);
			f->gpr[0] = (uint32_t)value;
			f->gpr[1] = value >> 32;
		}
	}
	return 0;
}

static struct fixture fresh(void)
{
	struct fixture f = {.gpr = {0x11223344, 0x55667788}, .dspsr = 0xa200041a,
		.midr = 0x411fd134, .mvfr0 = 0x10110222, .mvfr1 = 0x12111111, .fpexc = 0x40000700};
	for (unsigned int i = 0; i < 32; i++)
		f.d[i] = UINT64_C(0xfedcba9876543200) + i;
	return f;
}

static int transfer(struct fixture *f, const char *name, const char *raw,
	struct armv8_debugtui_vfp_write_result *result, bool *sent, bool *uncertain)
{
	struct armv8_debugtui_vfp_write_request request;
	assert(armv8_debugtui_vfp_write_request(name, raw, &request));
	struct armv8_debugtui_io io = {.context = f, .read_gpr = read_gpr,
		.write_gpr = write_gpr, .execute = execute};
	return armv8_debugtui_vfp_write_transfer(&io, &request, result, sent, uncertain);
}

int main(void)
{
	struct armv8_debugtui_vfp_write_request request;
	const char *invalid[] = {"s32", "s01", "S0", "s-1", "d32", "q16", "d01", "fpscr", "fpexc", "mvfr0", ""};
	for (unsigned int i = 0; i < sizeof(invalid) / sizeof(invalid[0]); i++)
		assert(!armv8_debugtui_vfp_write_request(invalid[i], "0x00000000", &request));
	const char *bad_raw[] = {"0", "0x0", "0x000000000", "0x0000000g", "0x1234567;", "0X12345678", "-0x12345678", "0x12345678\n"};
	for (unsigned int i = 0; i < sizeof(bad_raw) / sizeof(bad_raw[0]); i++)
		assert(!armv8_debugtui_vfp_write_request("s0", bad_raw[i], &request));
	/* Independently assembled VMOV Dn,R0,R1, low and high register banks. */
	assert(ARMV5_T_MCRR(11, 1, 0, 1, 0) == UINT32_C(0xec410b10));
	assert(ARMV5_T_MCRR(11, 3, 0, 1, 15) == UINT32_C(0xec410b3f));
	for (unsigned int kind = 0; kind < 3; kind++) {
		unsigned int count = kind == 2 ? 16 : 32;
		for (unsigned int index = 0; index < count; index++) {
			struct fixture f = fresh(), before = f;
			char name[4];
			snprintf(name, sizeof(name), "%c%u", "sdq"[kind], index);
			const char *raw = kind == 0 ? "0x7fa12345" : kind == 1 ? "0xfff0123456789abc" :
				"0x8123456789abcdef7ff0123456789abc";
			unsigned int d = kind == 0 ? index / 2 : kind == 1 ? index : index * 2;
			uint64_t expected[32]; memcpy(expected, before.d, sizeof(expected));
			if (kind == 0) {
				unsigned int shift = (index % 2) * 32;
				expected[d] = (expected[d] & ~(UINT64_C(0xffffffff) << shift)) | (UINT64_C(0x7fa12345) << shift);
			} else if (kind == 1) expected[d] = UINT64_C(0xfff0123456789abc);
			else { expected[d] = UINT64_C(0x7ff0123456789abc); expected[d + 1] = UINT64_C(0x8123456789abcdef); }
			struct armv8_debugtui_vfp_write_result result;
			bool sent = false, uncertain = true;
			assert(transfer(&f, name, raw, &result, &sent, &uncertain) == 0 && sent && !uncertain);
			assert(f.writes == (kind == 2 ? 2u : 1u) && f.operations == (kind == 2 ? 144u : 135u));
			assert(!memcmp(f.d, expected, sizeof(expected)) && !memcmp(f.gpr, before.gpr, sizeof(f.gpr)));
			unsigned int pair = d / 2;
			assert(result.before.words[0] == before.d[pair * 2] && result.before.words[1] == before.d[pair * 2 + 1]);
			assert(result.expected[0] == expected[pair * 2] && result.expected[1] == expected[pair * 2 + 1]);
			assert(!memcmp(result.expected, result.observed.words, sizeof(result.expected)));
		}
	}
	for (unsigned int failure = 1; failure <= 144; failure++) {
		struct fixture f = fresh(); f.fail_at = failure;
		struct armv8_debugtui_vfp_write_result result = {.expected = {11, 22}};
		bool sent = false, uncertain = false;
		assert(transfer(&f, "q15", "0x0123456789abcdeffedcba9876543210", &result, &sent, &uncertain) == -7);
		assert(uncertain && f.operations == failure && result.expected[0] == 11 && result.expected[1] == 22);
		assert(f.writes <= 2 && sent == (failure >= 68));
	}
	/* Failure after the first or second physical write: no replay or rollback. */
	for (unsigned int lane = 0; lane < 2; lane++) {
		struct fixture f = fresh(); f.fail_at = 68 + lane * 9; f.fail_after_commit = true;
		struct armv8_debugtui_vfp_write_result result = {.expected = {11, 22}};
		bool sent = false, uncertain = false;
		assert(transfer(&f, "q15", "0x0123456789abcdeffedcba9876543210", &result, &sent, &uncertain) == -7);
		assert(sent && uncertain && f.writes == lane + 1 && f.operations == f.fail_at);
		assert(f.d[30] == UINT64_C(0xfedcba9876543210));
		assert(f.d[31] == (lane ? UINT64_C(0x0123456789abcdef) : fresh().d[31]));
	}
	for (unsigned int changed = 1; changed <= 7; changed++) {
		struct fixture f = fresh();
		if (changed <= 5) f.changed = changed; else f.corrupt_restore = changed - 5;
		struct armv8_debugtui_vfp_write_result result = {.expected = {11, 22}};
		bool sent = false, uncertain = false;
		assert(transfer(&f, "s31", "0x80000000", &result, &sent, &uncertain) != 0);
		assert(sent && uncertain && f.writes == 1 && result.expected[0] == 11 && result.expected[1] == 22);
	}
	/* Check the entire pair, including a sibling outside the selected S lane. */
	for (unsigned int index = 0; index < 2; index++) {
		struct fixture f = fresh(); f.corrupt_data = index + 1;
		struct armv8_debugtui_vfp_write_result result;
		bool sent = false, uncertain = true;
		assert(transfer(&f, "s0", "0x80000000", &result, &sent, &uncertain) == DEBUGTUI_VFP_WRITE_MISMATCH);
		assert(sent && !uncertain && f.writes == 1 && !memcmp(f.gpr, fresh().gpr, sizeof(f.gpr)));
	}
	for (unsigned int refused = 0; refused < 8; refused++) {
		struct fixture f = fresh(), before;
		const char *name = "d16", *raw = "0x0123456789abcdef";
		if (refused < 2) f.dspsr = refused == 0 ? 0x10 : 0x13;
		if (refused == 2) f.hcptr = 1u << 10;
		if (refused == 3) f.fpexc &= ~(1u << 30);
		if (refused == 4 || refused == 5) {
			f.mvfr0 = 0x10110021; f.mvfr1 = 0x11000011;
			if (refused == 5) { name = "q0"; raw = "0x0123456789abcdeffedcba9876543210"; }
		}
		if (refused == 6) f.mvfr1 |= 2u << 12;
		if (refused == 7) f.midr ^= 1u << 4;
		before = f;
		struct armv8_debugtui_vfp_write_result result = {.expected = {11, 22}};
		bool sent = true, uncertain = true;
		assert(transfer(&f, name, raw, &result, &sent, &uncertain) > 0 && !sent && !uncertain);
		assert(f.writes == 0 && !memcmp(f.d, before.d, sizeof(f.d)) && !memcmp(f.gpr, before.gpr, sizeof(f.gpr)));
		assert(result.expected[0] == 11 && result.expected[1] == 22);
	}
	/* D16 SP-only still supports raw D and all 32 S views; no float arithmetic. */
	struct fixture sp = fresh(); sp.mvfr0 = 0x10110021; sp.mvfr1 = 0x11000011;
	struct armv8_debugtui_vfp_write_result result;
	bool sent = false, uncertain = true;
	assert(transfer(&sp, "s31", "0x7FA12345", &result, &sent, &uncertain) == 0 && sent && !uncertain);
	assert(sp.d[15] == ((fresh().d[15] & UINT64_C(0xffffffff)) | UINT64_C(0x7fa1234500000000)));
	puts("PASS: 80 S/D/Q views, raw NaN/sign bits, fresh siblings, 144 fault points, partial Q writes, controls and safe refusals");
	return 0;
}
