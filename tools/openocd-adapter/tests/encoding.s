/* SPDX-License-Identifier: GPL-2.0-or-later */
.syntax unified
.arch armv8-r
.thumb
.text
mrrc p15, 0, r0, r1, c14
isb sy
