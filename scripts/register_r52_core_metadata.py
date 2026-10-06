"""Bounded R52 identity/control/MPU facts, reviewed against 100026_0104_01_en.

Pages are one-based physical PDF pages. No target I/O or writer declarations.
Conflicting text is retained explicitly; revision and configuration are not guessed.
"""
import re
from register_timer_metadata import field

DOCUMENT = "Arm Cortex-R52 Processor Technical Reference Manual"
VERSION = "100026_0104_01_en, Issue 01"
COMMON = ("Halted physical core. The following EL rules describe normal execution; "
          "current Debug EL, execution state, authorization and applicable traps require "
          "separate backend evidence. Saved CPSR/DSPSR and configuration do not authorize a read. ")
EL1_MEMORY = ("EL1/EL2; EL1 reads are subject to HCR.TRVM. ")
EL2 = "EL2 only; lower EL access is not authorized by a saved Hyp CPSR."


def entry(encoding, section, page, description, fields, access, reset=None, confidence="high"):
    return dict(encoding=encoding, section=section, page=page, description=description,
                fields=fields, access_condition=COMMON + access, reset=reset, confidence=confidence)


def controls(hyp):
    level = "EL2" if hyp else "EL0/EL1"
    result = [
        field("M", 0, 1, f"Global {'EL2' if hyp else 'EL1'} MPU enable; region EN is separate.", [(0,"Disabled"),(1,"Enabled")]),
        field("A", 1, 1, f"Alignment fault checking at {level}."),
        field("C", 2, 1, f"Data cache enable at {level}; does not enable the MPU."),
        field("CP15BEN", 5, 1, f"Legacy CP15 barrier instruction enable at {level}; reset bit is 1."),
        field("ITD", 7, 1, f"Restrictions on IT instruction encodings at {level}."),
        field("SED", 8, 1, f"SETEND instruction disable at {level}."),
        field("I", 12, 1, f"Instruction cache enable at {level}."),
        field("BR", 17, 1, f"Background region enable for {'EL2' if hyp else 'EL1'} accesses."),
        field("WXN", 19, 1, "Writable MPU regions are treated as execute-never when set."),
        field("FI", 21, 1, "Fast interrupt configuration." if hyp else "Read-only copy of HSCTLR.FI.", access=None if hyp else "ro"),
        field("EE", 25, 1, "Exception endianness; reset depends on CFGENDIANESSx.", [(0,"LittleEndian"),(1,"BigEndian")]),
        field("TE", 30, 1, "Exception instruction state; reset depends on CFGTHUMBEXCEPTIONSx.", [(0,"A32"),(1,"T32")]),
    ]
    if not hyp:
        result += [field("nTWI",16,1,"EL0 WFI trapping to EL1; reset is 1."),
                   field("nTWE",18,1,"EL0 WFE trapping to EL1; reset is 1."),
                   field("UWXN",20,1,"Unprivileged-writable regions are execute-never for EL1 when set.")]
    return result


PERMISSIONS = [(0,"Disabled"),(1,"EL1Only"),(2,"Reserved"),(3,"EL0AndEL1")]
HCR_FIELDS = [
    field("VM",0,1,"EL2 MPU protection for EL0/EL1; HCR.DC also affects protection.",[(0,"DisabledUnlessDC"),(1,"Enabled")]),
    field("SWIO",1,1,"Set/way invalidation override, RES1 on R52.",[(1,"RES1")],access="ro"),
    *[field(name,bit,1,desc) for name,bit,desc in [
        ("FMO",3,"Physical FIQ mask override and virtual FIQ signaling."),
        ("IMO",4,"Physical IRQ mask override and virtual IRQ signaling."),
        ("AMO",5,"Physical asynchronous abort mask override and virtual abort signaling."),
        ("VF",6,"Virtual FIQ signal."),("VI",7,"Virtual IRQ signal."),("VA",8,"Virtual asynchronous abort signal."),
        ("FB",9,"Force broadcast has no effect on R52's single-core Inner Shareable domain."),
        ("DC",12,"Default cacheability for PMSA guests; does not force SCTLR.M to zero."),
        ("TWI",13,"Trap a suspending lower-EL WFI to EL2."),("TWE",14,"Trap a suspending lower-EL WFE to EL2."),
        ("TID0",15,"Trap ID group 0 reads."),("TID1",16,"Trap ID group 1 reads, including MPUIR."),
        ("TID2",17,"Trap cache ID group 2 accesses."),("TID3",18,"Trap ID group 3 reads."),
        ("TIDCP",20,"Trap implementation-defined CP15 accesses in the ranges of Table 4-93."),
        ("TAC",21,"Trap ACTLR accesses."),("TSW",22,"Trap data cache set/way maintenance."),
        ("TPC",23,"Trap data cache maintenance to point of coherency."),
        ("TPU",24,"Trap cache maintenance to point of unification."),
        ("TVM",26,"Trap writes to EL1 memory controls; does not describe read trapping."),
        ("TGE",27,"Route general exceptions to EL2 and apply Table 4-93's related controls."),
        ("HCD",29,"HVC instruction disable."),
        ("TRVM",30,"Trap reads of EL1 memory controls, including SCTLR, MAIR and MPU registers."),
    ]],
    field("BSU",10,2,"Minimum barrier shareability domain for lower ELs.",[(0,"Unchanged"),(1,"InnerShareable"),(2,"OuterShareable"),(3,"FullSystem")]),
]
SELECTOR = [field("REGION",0,5,"Capacity-dependent field: 16 regions use [3:0] and bit 4 is RES0; 20/24 use [4:0]. Valid selection is below the observed bank count; zero-capacity writes are UNPREDICTABLE.")]

R52_CORE_METADATA = {
    "midr": entry((0,0,0,0),"4.3.69, Tables 4-1/4-160, pp.52/169-170",169,
        "Physical processor identity. Source conflict: Table 4-1 reset is 0x411FD134, but the Revision row on p.170 says 0x5 for r1p4. Preserve observed variant/revision; no fixed reset or revision is asserted.",
        [field("Implementer",24,8,"Processor implementer code.",[(0x41,"Arm")]),field("Variant",20,4,"Major revision from the actual value."),
         field("Architecture",16,4,"Identification scheme.",[(15,"CPUIDScheme")]),field("PartNum",4,12,"Processor part number.",[(0xd13,"CortexR52")]),
         field("Revision",0,4,"Minor revision from the actual value; conflicting TRM revision text is not used as a constant.")],
        "EL1/EL2; HSTR.T0 can trap EL1 accesses.",confidence="medium"),
    "mpidr": entry((0,0,0,5),"4.3.78, Table 4-181, pp.182-183",182,
        "Physical affinity fields, not configured core indices. Source conflict: p.183's least/most-significant affinity wording is reversed relative to field positions; preserve Aff0/1/2 labels and raw positions without deriving topology.",
        [field("Aff0",0,8,"Affinity level 0; do not equate this to the configured core index."),field("Aff1",8,8,"CFGMPIDRAFF1 affinity."),
         field("Aff2",16,8,"CFGMPIDRAFF2 affinity."),field("MT",24,1,"Lowest affinity level multithreading indicator; R52 value is 0.",access="ro"),
         field("U",30,1,"Single-core-system indicator; R52 value 0 denotes a cluster member.",access="ro"),field("M",31,1,"R52 RES1.",access="ro")],
        "EL1/EL2; no traps or enables are listed for MPIDR in this TRM.",confidence="medium"),
    "sctlr": entry((0,1,0,0),"4.3.92, Table 4-212, pp.200-203",201,"EL1 memory and execution controls. Full reset is configuration-dependent; reading does not enable or alter controls.",controls(False),EL1_MEMORY+"HSTR.T1 can trap EL1 accesses."),
    "hsctlr": entry((4,1,0,0),"4.3.53, Table 4-130, pp.144-147",145,"EL2 memory and execution controls; no guessed full reset from configurable EE/TE bits.",controls(True),EL2),
    "cpacr": entry((0,1,0,2),"4.3.1, Table 4-36, pp.79-81",80,"Normal-execution SIMD/FP permissions at EL0/EL1; no effect on EL2 instructions, and not proof of external debugger permission.",
        [field("cp10",20,2,"SIMD/FP permission at EL0/EL1.",PERMISSIONS),field("cp11",22,2,"Ignored permission field; direct read is UNKNOWN when it differs from cp10. Preserve raw bits.",PERMISSIONS),
         field("TRCDIS",28,1,"RES0: R52 has no system-register trace macrocell interface.",access="ro"),field("ASEDIS",31,1,"Disable non-floating-point Advanced SIMD instructions at lower ELs.")],
        "EL1/EL2; EL1 reads can trap through HCPTR.TCPAC or HSTR.T1.",reset=0),
    "hcr": entry((4,1,1,0),"4.3.39, Table 4-93, pp.120-125",121,"Virtualization, interrupt and trap controls; reads never change mode, permission or MPU configuration.",HCR_FIELDS,EL2,reset=2),
    "mpuir": entry((0,0,0,4),"4.3.77, Tables 4-178/4-179, pp.181-182",181,"Observed EL1 MPU capacity: 16, 20 or 24, independent of EL2 capacity.",
        [field("nU",0,1,"Not-unified indicator, R52 is unified (0).",access="ro"),field("DREGION",8,8,"Actual EL1 region count.",[(n,f"Regions{n}") for n in (16,20,24)],"ro"),field("IREGION",16,8,"RES0; no separate instruction region bank.",access="ro")],"EL1/EL2; HCR.TID1 can trap an EL1 read."),
    "hmpuir": entry((4,0,0,4),"4.3.47, Tables 4-113/4-114, pp.135-136",136,"Observed EL2 MPU capacity: 0, 16, 20 or 24. Zero is a valid unimplemented bank, not a transport failure.",
        [field("REGION",0,8,"Actual EL2 region count.",[(n,f"Regions{n}") for n in (0,16,20,24)],"ro")],EL2),
    "prselr": entry((0,6,2,1),"4.3.87, Tables 4-201/4-202, pp.195-196",195,
        "EL1 indirect region selector. Source conflict: capacity headings say EL2 although this section and encoding identify PRSELR as EL1. Capacity comes from MPUIR, never HMPUIR. Direct indexed reads do not write this selector.",SELECTOR,
        EL1_MEMORY+"EL1 access additionally requires VSCTLR.MSA=0.",confidence="medium"),
    "hprselr": entry((4,6,2,1),"4.3.50, Tables 4-122/4-123, pp.139-140",140,"EL2 indirect region selector; validity and field width depend on HMPUIR. Direct indexed reads do not write it.",SELECTOR,EL2),
    "hprenr": entry((4,6,1,1),"4.3.46, Tables 4-110/4-111/4-112, pp.133-135",135,"EL2 region-enable bitmap; bits outside actual HMPUIR capacity are RAZ, never inferred as regions.",
        [field("ENABLES",0,24,"Bit n enables EL2 region n when implemented; bits above the actual count read as zero.")],EL2,reset=0),
}
for hyp in (False,True):
    for bank in (0,1):
        name=("h" if hyp else "")+f"mair{bank}"
        R52_CORE_METADATA[name]=entry((4 if hyp else 0,10,2,bank),
            "4.3.45, Tables 4-103/4-104, pp.131-133" if hyp else "4.3.70, Tables 4-162/4-163/4-164, pp.170-172",
            132 if hyp else 171,
            f"{'EL2' if hyp else 'EL1'} MPU attribute bytes {bank*4}-{bank*4+3}; reset UNKNOWN. Device/reserved/Normal encodings remain distinct; R52 ignores the transient hint.",
            [field(f"Attr{i+bank*4}",i*8,8,"Attribute byte selected by region AttrIndx; Device when outer nibble is zero, otherwise Normal. Reserved encodings stay UNPREDICTABLE.") for i in range(4)],
            EL2 if hyp else EL1_MEMORY+"HSTR.T10 can trap EL1 accesses.")


def r52_core_metadata(name):
    if name in R52_CORE_METADATA:
        return R52_CORE_METADATA[name]
    match=re.fullmatch(r"(h?)(prbar|prlar)(\d+)",name)
    if not match:
        return None
    hyp,limit,index=bool(match[1]),match[2]=="prlar",int(match[3])
    if index>=24:
        raise ValueError("R52 direct MPU metadata only covers the adapted 0-23 range")
    level=2 if hyp else 1
    if limit:
        fields=[field("LIMIT",6,26,"Upper inclusive limit bits; append 0x3F for the address. Reset UNKNOWN."),
                field("AttrIndx",1,3,f"Selects one of eight {'HMAIR' if hyp else 'MAIR'} attribute bytes."),
                field("EN",0,1,"Region enable resets to 0; the remaining bits reset UNKNOWN.",[(0,"Disabled"),(1,"Enabled")])]
    else:
        upper,lower=("EL2","EL0/EL1") if hyp else ("EL1","EL0")
        fields=[field("BASE",6,26,"Lower inclusive address bits; append six zero bits for the aligned base. Reset UNKNOWN."),
                field("SH",3,2,"Normal-memory shareability; Device and Normal non-cacheable behavior is specified separately.",[(0,"NonShareable"),(1,"UNPREDICTABLE"),(2,"OuterShareable"),(3,"InnerShareable")]),
                field("AP",1,2,f"Data access at {upper} and {lower}; executable permission is separate.",[(0,f"{upper}RW_{lower}None"),(1,"AllRW"),(2,f"{upper}RO_{lower}None"),(3,"AllRO")]),
                field("XN",0,1,"Execute-never; independent of the AP data permission.",[(0,"ExecutableIfOtherwisePermitted"),(1,"ExecuteNever")])]
    section=("4.3.49, Table 4-120, pp.138-139" if limit else "4.3.48, Tables 4-116/4-117/4-118, pp.136-138") if hyp else ("4.3.86, Table 4-199, pp.193-194" if limit else "4.3.85, Tables 4-195/4-196/4-197, pp.192-193")
    page=(139 if limit else 137) if hyp else (194 if limit else 192)
    return entry(((4 if hyp else 0)+index//16,6,8+(index%16)//2,4*(index%2)+int(limit)),section,page,
        f"Direct EL{level} MPU region {index} {'inclusive limit/attribute index/enable' if limit else 'base/permissions/shareability/execute-never'}; exists only below its own observed bank capacity. No selector write, no fixed full reset.",
        fields,EL2 if hyp else EL1_MEMORY+"EL1 access requires VSCTLR.MSA=0.")
