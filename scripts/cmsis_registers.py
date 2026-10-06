"""Bounded offline CMSIS header conversion, with explicitly cited read policies.

No preprocessor/compiler or eval is used. Unknown expressions fail generation.
Legacy GDB definitions remain separate from the newly sourced PPB definitions.
"""
from pathlib import Path
import ast
import hashlib
import json
import re
import tomllib

INPUT = Path(__file__).resolve().parents[1] / "third_party" / "cmsis-core"
ARMV7M = "https://documentation-service.arm.com/static/606dc36485368c4c2b1bf62f"
q = json.dumps
READ_FIELD_EXCLUDES = {
    "SCB_CFSR": {"MEMFAULTSR", "BUSFAULTSR", "USGFAULTSR"},  # aggregate masks, use fault bits
    "DCB_DHCSR": {"DBGKEY"},  # write view overlaps the read status bits
    "SCB_AIRCR": {"VECTKEY", "ENDIANESS"},  # write key and deprecated spelling alias
    "MPU_RASR": {"ATTRS"},  # aggregate mask, use attribute bit fields
}


def inline(value):
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, str):
        return q(value)
    if isinstance(value, int):
        return str(value)
    if isinstance(value, list):
        return "[" + ", ".join(inline(v) for v in value) + "]"
    if isinstance(value, dict):
        return "{ " + ", ".join(f"{k} = {inline(v)}" for k, v in value.items()) + " }"
    raise ValueError(f"Unsupported TOML value {value!r}")


class Header:
    def __init__(self, name, lock):
        data = (INPUT / name).read_bytes()
        if hashlib.sha256(data).hexdigest() != lock["files"][name]:
            raise ValueError(f"Pinned CMSIS input changed: {name}")
        self.name, self.lock = name, lock
        self.text = data.decode("utf-8")
        self.lines = self.text.splitlines()
        self.macros = {}
        self.ambiguous_macros = set()
        for number, line in enumerate(self.lines, 1):
            found = re.match(r"\s*#define\s+(\w+)\s+(.+)", line)
            if found and not found[2].startswith("\\"):
                if found[1] in self.macros:
                    previous = re.sub(r"/\*.*?\*/", "", self.macros[found[1]][0]).strip()
                    current = re.sub(r"/\*.*?\*/", "", found[2]).strip()
                    if previous != current:
                        self.ambiguous_macros.add(found[1])
                self.macros[found[1]] = (found[2], number)

    def number(self, expression, seen=()):
        expression = re.sub(r"/\*.*?\*/", "", expression)
        expression = re.sub(r"\b(0[xX][\da-fA-F]+|\d+)(?:ULL|UL|LU|U|L)\b", r"\1", expression)
        def visit(node):
            if isinstance(node, ast.Constant) and type(node.value) is int:
                return node.value
            if isinstance(node, ast.Name) and node.id in self.macros and node.id not in seen:
                if node.id in self.ambiguous_macros:
                    raise ValueError(f"Conditional CMSIS macro requires explicit policy: {node.id}")
                return self.number(self.macros[node.id][0], (*seen, node.id))
            if isinstance(node, ast.BinOp):
                left, right = visit(node.left), visit(node.right)
                if isinstance(node.op, ast.Add):
                    return left + right
                if isinstance(node.op, ast.LShift) and 0 <= right <= 31:
                    return left << right
                if isinstance(node.op, ast.BitOr):
                    return left | right
            raise ValueError(f"Unsupported CMSIS integer expression: {expression}")
        return visit(ast.parse(expression.strip(), mode="eval").body)

    def source(self, section, line):
        return dict(document=f"CMSIS-Core {self.name}", version=self.lock["version"],
                    number=self.lock["commit"], section=section, line=line,
                    url=f'https://github.com/ARM-software/CMSIS_6/blob/{self.lock["commit"]}/CMSIS/Core/Include/{self.name}#L{line}')

    def members(self, typename):
        found = re.search(r"typedef struct\s*\{([^{}]+)\}\s*" + re.escape(typename) + r";", self.text)
        if not found:
            raise ValueError(f"CMSIS structure missing: {self.name}:{typename}")
        result = {}
        for match in re.finditer(r"__(IM|IOM|OM)\s+uint(8|32)_t\s+(\w+)(?:\[(\d+)U?\])?;[^\n]*?Offset:\s*(0x[\da-fA-F]+)[^\n]*", found[1]):
            absolute = found.start(1) + match.start()
            line = self.text.count("\n", 0, absolute) + 1
            result[match[3]] = dict(offset=int(match[5], 16), bits=int(match[2]),
                                    count=int(match[4] or 1), access={"IM":"ro","IOM":"rw","OM":"wo"}[match[1]],
                                    source=self.source(f"{typename}.{match[3]} and field masks", line))
        return result

    def fields(self, prefix, exclude=()):
        fields = []
        used = 0
        for name in self.macros:
            if not name.startswith(prefix + "_") or not name.endswith("_Pos"):
                continue
            field = name[len(prefix)+1:-4]
            if field in exclude:
                continue
            offset = self.number(name)
            value = self.number(name[:-4] + "_Msk")
            mask = value >> offset
            if not mask or mask & (mask + 1) or value != mask << offset or value >= 1 << 32:
                raise ValueError(f"Non-contiguous/invalid CMSIS field: {name}")
            if used & value:
                raise ValueError(f"Overlapping CMSIS fields: {name}")
            used |= value
            fields.append(dict(name=field, segments=[dict(offset=offset, width=mask.bit_length())]))
        return sorted(fields, key=lambda f: f["segments"][0]["offset"])


def condition(reg, field, op, value):
    return dict(reg=reg, field=field, op=op, value=value)


def ppb(header, typename, prefix, member, group, **options):
    base = header.number("SCS_BASE" if prefix == "SCnSCB" else prefix + "_BASE")
    definition = header.members(typename)[member]
    revision_dependent_vtor = header.name == "core_cm3.h" and prefix == "SCB" and member == "VTOR"
    fields = [] if revision_dependent_vtor else header.fields(
        prefix + "_" + member, READ_FIELD_EXCLUDES.get(prefix + "_" + member, ()))
    result = dict(id=f"{prefix.lower()}.{member.lower()}", name=f"{prefix}.{member}",
                  group=group, bits=definition["bits"], access=definition["access"], scope="core",
                  description=f"{prefix} {member}; reset and implementation-specific values are not assumed.",
                  reader=dict(kind="core_private", address=base+definition["offset"]),
                  access_rule=dict(need_halt=False), source=definition["source"], confidence="high", fields=fields)
    result.update(options)
    if revision_dependent_vtor:
        result["fields_missing"] = True
        result["description"] += " TBLBASE/TBLOFF layout depends on __CM3_REV before/after r2p1. Preserve raw VTOR; no field layout is chosen without observed revision."
    return result


def manual(section, page):
    return dict(document="Armv7-M Architecture Reference Manual", version="E.e",
                number="DDI 0403E.e", section=section, page=page, url=ARMV7M)


def common(header):
    definitions = []
    for name in ["CPUID", "ICSR", "VTOR", "AIRCR", "SCR", "CCR", "SHCSR", "CFSR", "HFSR", "DFSR", "MMFAR", "BFAR", "AFSR"]:
        definitions.append(ppb(header, "SCB_Type", "SCB", name, "scb"))
    definitions.append(ppb(header, "SCnSCB_Type", "SCnSCB", "ICTR", "nvic", id="scs.ictr", name="ICTR"))
    for name in ["CTRL", "LOAD", "VAL", "CALIB"]:
        item = ppb(header, "SysTick_Type", "SysTick", name, "systick")
        if name == "CTRL":
            item.update(read_side_effect=True,
                        description="Conservative manual-only policy. Software reads clear COUNTFLAG; architected debugger reads preserve it, but this transport has no verified debug-access attribute. DDI 0403E.e B3.3.2, physical PDF page 621.")
        definitions.append(item)
    for name in ["TYPE", "CTRL", "RNR", "RBAR", "RASR"]:
        item = ppb(header, "MPU_Type", "MPU", name, "m_mpu")
        if name != "TYPE":
            item["present_if"] = condition("mpu.type", "DREGION", "gt", 0)
        if name in ["RBAR", "RASR"]:
            item["description"] += " Value of the currently selected region; reading does not select another region."
        definitions.append(item)
    for name in ["DHCSR", "DEMCR"]:
        item = ppb(header, "DCB_Type", "DCB", name, "m_debug")
        if name == "DHCSR":
            item.update(read_side_effect=True,
                        description="Manual single read only; S_RESET_ST and S_RETIRE_ST are cleared by a read. DDI 0403E.e C1.6.1, physical PDF page 701. Core status tracking remains owned by GDB/OpenOCD.")
        definitions.append(item)
    dwt = ppb(header, "DWT_Type", "DWT", "CTRL", "m_debug")
    dwt["access_rule"]["need_enable"] = condition("dcb.demcr", "TRCENA", "eq", 1)
    definitions.append(dwt)
    definitions.append(dict(id="fpb.ctrl", name="FP_CTRL", group="m_debug", bits=32,
                            access="rw", scope="core", reader=dict(kind="core_private", address=0xE0002000),
                            access_rule=dict(need_halt=False), source=manual("C1.11.3 FP_CTRL", 757), confidence="high",
                            description="FPB identity and capacity. Instruction comparator count spans two discontiguous fields; reset varies with implementation. No comparator or enable writes.",
                            fields=[dict(name="ENABLE",segments=[dict(offset=0,width=1)]),
                                    dict(name="NUM_CODE",segments=[dict(offset=4,width=4),dict(offset=12,width=3)]),
                                    dict(name="NUM_LIT",segments=[dict(offset=8,width=4)]),
                                    dict(name="REV",segments=[dict(offset=28,width=4)])]))
    nvic = header.members("NVIC_Type")
    for name in ["ISER", "ICER", "ISPR", "ICPR", "IABR", "IPR"]:
        item = nvic[name]
        for index in range(item["count"]):
            definitions.append(dict(id=f"nvic.{name.lower()}{index}", name=f"NVIC.{name}{index}", group="nvic",
                                    bits=item["bits"], access="ro" if name == "IABR" else item["access"], scope="core",
                                    reader=dict(kind="core_private", address=header.number("NVIC_BASE")+item["offset"]+index*(item["bits"]//8)),
                                    access_rule=dict(need_halt=False), source=item["source"], confidence="high",
                                    description="Interrupt bank/priority byte; ICTR supplies only an upper bound. Actual interrupt availability and priority bits require SVD or explicit configuration. No writer or write-based priority probe.",
                                    present_if=condition("scs.ictr", "INTLINESNUM", "ge", index if name != "IPR" else index//32)))
    return definitions


def deltas(header, baseline):
    result = []
    for name in ["CPUID", "ICSR", "VTOR", "AIRCR", "SCR", "CCR", "SHCSR", "CFSR", "HFSR", "DFSR", "MMFAR", "BFAR", "AFSR"]:
        item = ppb(header, "SCB_Type", "SCB", name, "scb")
        parent = next(r for r in baseline if r["id"] == item["id"])
        if item["fields"] != parent["fields"]:
            result.append(dict(item, override=True))
    cpacr = ppb(header, "SCB_Type", "SCB", "CPACR", "m_fpu")
    # CMSIS supplies the address but no CPACR masks in these pinned headers.
    # The M-specific user guides independently specify CP10[21:20]/CP11[23:22].
    m7 = header.name == "core_cm7.h"
    cpacr["source"] = dict(document=f"Cortex-M{7 if m7 else 4} Devices Generic User Guide",
                           version="B" if m7 else "A", number="DUI 0646B" if m7 else "DUI 0553A",
                           section="4.7.1, Table 4-59, printed page 4-56" if m7 else "4.6.1, Table 4-50, printed page 4-48",
                           page=287 if m7 else 264,
                           url="https://documentation-service.arm.com/static/61efd6602dd99944d051417b" if m7 else "https://documentation-service.arm.com/static/5f2ac4ab60a93e65927bbdbf")
    cpacr["fields"] = [dict(name=f"CP{n}", segments=[dict(offset=2*n, width=2)],
                           description="Processor FP instruction access permission; this is not debugger regfile authorization. Reserved encoding stays visible; no automatic enable.",
                           enums=[dict(value=str(value), name=name) for value, name in
                                  enumerate(["Denied", "PrivilegedOnly", "ReservedUnpredictable", "FullAccess"])])
                        for n in (10,11)]
    cpacr["description"] = "CP10/CP11 instruction access configuration, with CMSIS address and Arm M-profile field definitions. Read-only debugger view; no CPU-mode inference or automatic FPU enable."
    result.append(cpacr)
    for name in ["MVFR0", "MVFR1", "MVFR2", "FPCCR", "FPCAR", "FPDSCR"]:
        item = ppb(header, "FPU_Type", "FPU", name, "m_fpu")
        if name not in ["MVFR0", "MVFR1", "MVFR2"]:
            item["present_if"] = condition("fpu.mvfr0", "SIMDReg", "eq", 1)
        result.append(item)
    if header.name == "core_cm7.h":
        for name in ["CLIDR", "CTR", "CCSIDR", "CSSELR", "CACR", "ITCMCR", "DTCMCR", "AHBPCR", "AHBSCR"]:
            result.append(ppb(header, "SCB_Type", "SCB", name, "m_cache"))
        for name in ["ICIALLU", "ICIMVAU", "DCIMVAC", "DCISW", "DCCMVAU", "DCCMVAC", "DCCSW", "DCCIMVAC", "DCCISW"]:
            result.append(ppb(header, "SCB_Type", "SCB", name, "m_cache",
                              access="wo", description="Cache maintenance command definition only. This read-only feature never issues it."))
    return result


def write_catalogue(path, cpu, architecture, definitions, groups=(), parents=()):
    lines = ["# Generated offline by scripts/cmsis_registers.py; see third_party/cmsis-core/source-lock.json.",
             "# CMSIS-derived facts retain upstream Apache-2.0 copyright notices in third_party/cmsis-core.",
             "# Version 1 preserves legacy GDB metadata whose manual source remains Unknown.",
             "version = 1", f"cpu = {q(cpu)}", f"architecture = {q(architecture)}",
             'description = "Read-only system catalogue; definitions do not prove target identity or transport support."']
    if parents:
        lines.append(f"extends = {inline(list(parents))}")
    for group in groups:
        lines.extend(["", "[[groups]]", *(f"{k} = {inline(v)}" for k,v in group.items())])
    for register in definitions:
        lines.extend(["", "[[registers]]", *(f"{k} = {inline(v)}" for k,v in register.items())])
    path.write_text("\n".join(lines)+"\n", encoding="utf-8", newline="\n")


def generate_m_catalogues(root, legacy_text):
    lock = json.loads((INPUT / "source-lock.json").read_text(encoding="utf-8"))
    for filename, digest in lock["files"].items():
        if hashlib.sha256((INPUT/filename).read_bytes()).hexdigest() != digest:
            raise ValueError(f"Pinned CMSIS input changed: {filename}")
    headers = [Header(f"core_cm{number}.h", lock) for number in (3,4,7)]
    legacy = tomllib.loads(legacy_text)
    core = [r for r in legacy["registers"] if r["group"] == "core"]
    common_regs = common(headers[0])
    groups = [dict(id="core",name="Core"),dict(id="simd",name="SIMD"),dict(id="system",name="System")]
    groups += [dict(id=id,name=name,parent="system") for id,name in
               [("scb","SCB and fault status"),("nvic","NVIC"),("systick","SysTick"),("m_mpu","MPU"),("m_debug","Debug / DWT / FPB")]]
    write_catalogue(root/"armv7m-common.toml", "armv7m-common", "armv7-m", core+common_regs, groups)
    for number, header in zip((3,4,7), headers):
        definitions, extra_groups = [], []
        if number != 3:
            definitions = deltas(header, common_regs)
            extra_groups = [dict(id="vfp",name="Floating point",parent="simd"),dict(id="m_fpu",name="FPU configuration / identity",parent="system")]
            for old in legacy["registers"]:
                if old["group"] != "core":
                    definitions.append(dict(old, present_if=condition("fpu.mvfr0", "SIMDReg", "eq", 1)))
        if number == 7:
            extra_groups.append(dict(id="m_cache",name="Cache / TCM configuration",parent="system"))
        write_catalogue(root/f"cortex-m{number}.toml",f"cortex-m{number}","armv7-m" if number == 3 else "armv7e-m",
                        definitions, extra_groups, ["armv7m-common"])
