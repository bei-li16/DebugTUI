/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Tests the exact transaction header compiled into the patched OpenOCD. */
#include <assert.h>
#include <stdio.h>
#include <string.h>
#include "armv8_debugtui.h"

struct fixture {
	uint32_t gpr[2];
	unsigned int operations, execute_count, fail_at;
	bool corrupt_restore;
	uint32_t opcode;
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
	f->gpr[reg] = value ^ (f->corrupt_restore && f->execute_count && reg == 0 ? 1 : 0);
	return 0;
}

static int execute(void *context, uint32_t opcode)
{
	struct fixture *f = context;
	if (++f->operations == f->fail_at)
		return -7;
	f->execute_count++;
	f->opcode = opcode;
	if ((opcode & UINT32_C(0xfff00000)) == UINT32_C(0xec500000)) {
		f->gpr[0] = 0x76543210;
		f->gpr[1] = 0xfedcba98;
	} else if (opcode == ARMV4_5_MRC(15, 0, 0, 1, 0, 0)) {
		f->gpr[0] = 0x76543210;
	} else {
		assert(opcode == ARMV4_5_MCR(15, 0, 0, 6, 2, 1));
		assert(f->gpr[0] == 0x12345678);
	}
	return 0;
}

int main(void)
{
	for (unsigned int failure = 0; failure <= 9; failure++) {
		struct fixture f = {.gpr = {0x11112222, 0x33334444}, .fail_at = failure};
		const struct armv8_debugtui_io io = {
			.context = &f, .read_gpr = read_gpr, .write_gpr = write_gpr, .execute = execute,
		};
		uint64_t value = UINT64_C(0x123456789abcdef0);
		int retval = armv8_debugtui_mrrc_transfer(&io, 15, 0, 14, &value);
		if (failure) {
			assert(retval == -7);
			assert(f.operations == failure); /* No rollback, retry, or extra opcode. */
			assert(value == UINT64_C(0x123456789abcdef0)); /* Never publish partial data. */
		} else {
			assert(retval == 0 && f.operations == 9 && f.execute_count == 1);
			assert(f.opcode == UINT32_C(0xec510f0e));
			assert(value == UINT64_C(0xfedcba9876543210));
			assert(f.gpr[0] == 0x11112222 && f.gpr[1] == 0x33334444);
		}
	}
	struct fixture f = {.gpr = {123, 456}, .corrupt_restore = true};
	const struct armv8_debugtui_io io = {
		.context = &f, .read_gpr = read_gpr, .write_gpr = write_gpr, .execute = execute,
	};
	uint64_t value = UINT64_MAX;
	assert(armv8_debugtui_mrrc_transfer(&io, 15, 15, 15, &value) != 0);
	assert(f.operations == 8 && value == UINT64_MAX);
	assert(armv8_debugtui_mrrc_encoding_valid(0, 0, 0));
	assert(armv8_debugtui_mrrc_encoding_valid(15, 15, 15));
	assert(!armv8_debugtui_mrrc_encoding_valid(16, 0, 0));
	assert(!armv8_debugtui_mrrc_encoding_valid(0, 16, 0));
	assert(!armv8_debugtui_mrrc_encoding_valid(0, 0, 16));
	for (unsigned int write = 0; write < 2; write++) {
		for (unsigned int failure = 0; failure <= 5; failure++) {
			struct fixture g = {.gpr = {0xaaaa5555, 0x5555aaaa}, .fail_at = failure};
			const struct armv8_debugtui_io scalar_io = {
				.context = &g, .read_gpr = read_gpr, .write_gpr = write_gpr, .execute = execute,
			};
			uint32_t scalar = 0x12345678;
			uint32_t opcode = write ? ARMV4_5_MCR(15, 0, 0, 6, 2, 1) : ARMV4_5_MRC(15, 0, 0, 1, 0, 0);
			int status = armv8_debugtui_mrc_mcr_transfer(&scalar_io, opcode, write, &scalar);
			if (failure) {
				assert(status == -7 && g.operations == failure && scalar == 0x12345678);
			} else {
				assert(status == 0 && g.operations == 5 && g.execute_count == 1);
				assert(g.gpr[0] == 0xaaaa5555 && g.gpr[1] == 0x5555aaaa);
				assert(scalar == (write ? 0x12345678 : 0x76543210));
			}
		}
	}
	for (unsigned int write = 0; write < 2; write++) {
		struct fixture bad = {.gpr = {0xaaaa5555, 0x5555aaaa}, .corrupt_restore = true};
		const struct armv8_debugtui_io bad_io = {
			.context = &bad, .read_gpr = read_gpr, .write_gpr = write_gpr, .execute = execute,
		};
		uint32_t scalar = 0x12345678;
		uint32_t opcode = write ? ARMV4_5_MCR(15, 0, 0, 6, 2, 1) : ARMV4_5_MRC(15, 0, 0, 1, 0, 0);
		assert(armv8_debugtui_mrc_mcr_transfer(&bad_io, opcode, write, &scalar) != 0);
		assert(bad.operations == 5 && scalar == 0x12345678);
	}
	puts("PASS: one MRRC, full high word, physical scratch restoration, 19 fail-stop points, restore mismatch, encoding bounds, preserved MRC/MCR");
	return 0;
}
