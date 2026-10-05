"""R52 GIC interface metadata, TRM 100026_0104_01_en 10.3.3-10.3.5.

Physical ICC, Hyp ICH and EL2-backed virtual AP aliases remain distinct.
No shared encoding is used to masquerade an EL2 physical read as EL1 ICV.
"""
from register_timer_metadata import field

ACCESS = ("Halted physical core. The observational adapter proves current Debug EL2, "
          "external R52 identity, interface enables and independent physical/virtual capacities. "
          "Below EL2 HCR.IMO/FMO can redirect ICC to ICV and HSTR/ICH_HCR can trap; "
          "permission remains Unknown. No interrupt acknowledgement, enable or control writes.")
GIC_ENCODINGS = {
    "icc_ctlr": (0, 12, 12, 4),
    "icc_sre": (0, 12, 12, 5),
    "icc_hsre": (4, 12, 9, 5),
    "icc_pmr": (0, 4, 6, 0),
    "icc_rpr": (0, 12, 11, 3),
    "icc_bpr0": (0, 12, 8, 3),
    "icc_bpr1": (0, 12, 12, 3),
    "icc_igrpen0": (0, 12, 12, 6),
    "icc_igrpen1": (0, 12, 12, 7),
    "icc_hppir0": (0, 12, 8, 2),
    "icc_hppir1": (0, 12, 12, 2),
    "icc_ap0r0": (0, 12, 8, 4),
    "icc_ap0r1": (0, 12, 8, 5),
    "icc_ap0r2": (0, 12, 8, 6),
    "icc_ap0r3": (0, 12, 8, 7),
    "icc_ap1r0": (0, 12, 9, 0),
    "icc_ap1r1": (0, 12, 9, 1),
    "icc_ap1r2": (0, 12, 9, 2),
    "icc_ap1r3": (0, 12, 9, 3),
    "ich_vtr": (4, 12, 11, 1),
    "ich_hcr": (4, 12, 11, 0),
    "ich_misr": (4, 12, 11, 2),
    "ich_eisr": (4, 12, 11, 3),
    "ich_elrsr": (4, 12, 11, 5),
    "ich_vmcr": (4, 12, 11, 7),
    "ich_ap0r0": (4, 12, 8, 0),
    "ich_ap0r1": (4, 12, 8, 1),
    "ich_ap0r2": (4, 12, 8, 2),
    "ich_ap0r3": (4, 12, 8, 3),
    "ich_ap1r0": (4, 12, 9, 0),
    "ich_ap1r1": (4, 12, 9, 1),
    "ich_ap1r2": (4, 12, 9, 2),
    "ich_ap1r3": (4, 12, 9, 3),
    "ich_lr0": (4, 12, 12, 0),
    "ich_lr1": (4, 12, 12, 1),
    "ich_lr2": (4, 12, 12, 2),
    "ich_lr3": (4, 12, 12, 3),
    "ich_lrc0": (4, 12, 14, 0),
    "ich_lrc1": (4, 12, 14, 1),
    "ich_lrc2": (4, 12, 14, 2),
    "ich_lrc3": (4, 12, 14, 3),
    "icc_iar0": (0, 12, 8, 0),
    "icc_iar1": (0, 12, 12, 0),
}
GIC_METADATA = {}
for name in GIC_ENCODINGS:
    hyp = name.startswith("ich_")
    description = ("Hyp control/backing register " if hyp else "Physical CPU interface register ") + name.upper() + ". Observing does not change interrupt state."
    conditions = [("gic.system_interface", 1, 1)]
    if "_ap" in name:
        n = int(name[-1]); minimum = [5, 6, 7, 7][n]
        conditions += [("icv.virtual.prebits" if hyp else "icc.physical.prebits", minimum)]
        description = ("Virtual active-priority backing storage in ICH; aliases ICV at EL1. " if hyp else "Physical active-priority bitmap; independent of ICH_VTR virtual capacity. ")
        description += "R52 implements only bank index 0; indices 1-3 remain architectural definitions and are never probed after capacity proves absence. Bitmap bits represent priorities, not interrupt IDs; BPR controls effective grouping."
    if name.startswith(("ich_lr", "ich_lrc")):
        conditions += [("ich.list_registers", int(name[-1])+1)]
        description = ("32-bit virtual interrupt ID storage in ICH_LR; upper ID bits are RAZ/WI on R52. " if not name.startswith("ich_lrc") else "Separate 32-bit ICH_LRC control/physical-ID part of one virtual interrupt list entry. ")
        description += "LR and LRC are separately sampled registers on R52; no atomic 64-bit pair is synthesized."
    access = "ro" if name in ["icc_hsre","icc_rpr","icc_hppir0","icc_hppir1","ich_vtr","ich_misr","ich_eisr","ich_elrsr","icc_iar0","icc_iar1"] else "rw"
    GIC_METADATA[name] = dict(description=description, fields=[field("Raw",0,32,"Raw 32-bit register; reserved bits are preserved, not interpreted.",access=access)], conditions=conditions, access=access, group="gic_ich" if hyp else "gic_icc",access_condition=ACCESS)
GIC_METADATA["icc_ctlr"]["description"] = "Physical ICC control at current Debug EL2. EL1 reads can instead be ICV_CTLR under HCR.IMO/FMO; stopped CPSR is insufficient to identify the sampled interface."
GIC_METADATA["icc_ctlr"]["fields"] = [field("CBPR",0,1,"Use Group 0 binary point for both groups."),field("EOImode",1,1,"Separate priority drop and deactivation; the reader performs neither."),field("PRIbits",8,3,"Physical priority bits minus one; R52 encodes 4 for five bits.",access="ro"),field("IDbits",11,3,"Physical interrupt-ID width encoding; R52 encodes zero for 16 interface bits.",access="ro")]
GIC_METADATA["ich_vtr"]["description"] = "Hyp virtual-interface capacity: five virtual priority/preemption bits and four list entries on R52. Never used as physical ICC AP capacity."
GIC_METADATA["ich_vtr"]["fields"] = [field("ListRegs",0,5,"Implemented list entries minus one; R52 encodes three.",access="ro"),field("TDS",19,1,"Separate ICC_DIR trapping support.",access="ro"),field("nV4",20,1,"No GICv4 direct virtual interrupt injection.",access="ro"),field("IDbits",23,3,"Virtual interrupt-ID width encoding.",access="ro"),field("PREbits",26,3,"Virtual preemption bits minus one; independent of physical ICC.",access="ro"),field("PRIbits",29,3,"Virtual priority bits minus one; independent of physical ICC.",access="ro")]
for name in ["icc_sre","icc_hsre"]:
    GIC_METADATA[name]["description"] = "R52 system-register interface and bypass enables are RAO/WI; observation does not enable the interface."
    GIC_METADATA[name]["fields"] = [field("SRE",0,1,"System-register interface enabled, RAO/WI on R52.",access="ro"),field("DFB",1,1,"FIQ bypass disabled, RAO/WI.",access="ro"),field("DIB",2,1,"IRQ bypass disabled, RAO/WI.",access="ro")]
    if name=="icc_hsre": GIC_METADATA[name]["fields"] += [field("Enable",3,1,"Lower exception-level access enabled, RAO/WI; other traps still apply.",access="ro")]
for name in ["icc_pmr","icc_rpr"]:
    GIC_METADATA[name]["fields"] = [field("Priority",3,5,"Implemented priority bits; lower values have higher priority. RPR 0xff means no active physical interrupt.",access=GIC_METADATA[name]["access"])]
for name in ["icc_bpr0","icc_bpr1"]:
    GIC_METADATA[name]["fields"] = [field("BinaryPoint",0,3,"Priority-group/subpriority split; does not change maximum implemented AP capacity.")]
for name in ["icc_hppir0","icc_hppir1","icc_iar0","icc_iar1"]:
    GIC_METADATA[name]["fields"] = [field("INTID",0,16,"Interrupt ID; 1023 is spurious/no pending interrupt.",access="ro")]
    if name.startswith("icc_iar"):
        GIC_METADATA[name]["description"] = "Read acknowledges an interrupt and changes pending/active/priority state. Never read automatically; the observational adapter rejects even manual acknowledgement requests. A separate reviewed acknowledgement path is not implemented."
for name in ["icc_igrpen0","icc_igrpen1"]:
    GIC_METADATA[name]["fields"] = [field("Enable",0,1,"Group interrupt signalling enable; never changed by observation.")]
for name in ["ich_eisr","ich_elrsr"]:
    GIC_METADATA[name]["fields"] = [field("ListBitmap",0,4,"Status bits for four implemented list entries.",access="ro")]
for n in range(4):
    GIC_METADATA[f"ich_lr{n}"]["fields"] = [field("vINTID",0,16,"Virtual interrupt identifier, separate from the LRC control sample.")]
    GIC_METADATA[f"ich_lrc{n}"]["fields"] = [field("PhysicalIdOrEOI",0,10,"HW=1: physical INTID; HW=0: bit 9 is EOI maintenance control, other bits reserved."),field("Priority",19,5,"Virtual interrupt priority."),field("Group",28,1,"Virtual interrupt group."),field("HW",29,1,"Hardware-backed virtual interrupt."),field("State",30,2,"Virtual interrupt state; observing does not acknowledge or deactivate.")]
for bank in range(2):
    for n in range(4):
        name=f"icv_ap{bank}r{n}"
        data=GIC_METADATA[f"ich_ap{bank}r{n}"]
        GIC_METADATA[name] = dict(data,group="gic_icv",description="Virtual active-priority alias from the same ICH backing sample at EL2. No ICV accessor is executed at EL2 and no Guest stack-frame value is substituted.")

for name in ["icc_eoir0","icc_eoir1","icc_dir","icc_sgi0r","icc_sgi1r","icc_asgi1r"]:
    GIC_METADATA[name] = dict(group="gic_icc",access="wo",fields=[],conditions=[("gic.system_interface",1,1)],access_condition="Write only; no observational reader or interrupt-control writer is provided.",description="Write-only interrupt priority-drop, deactivation or SGI command. Never read; the 64-bit SGI commands use MCRR for writes, not MRRC reads. No writer is enabled by this catalogue.")
