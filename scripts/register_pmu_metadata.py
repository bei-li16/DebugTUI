"""Read-only R52 PMU metadata, TRM 100026_0104_01_en 13.1-13.4.

Four 32-bit event counters, one 64-bit cycle counter. Normal access rules
are DDI0568A.c Table E1-1/HDCR; Debug keeps current EL/trap permissions.
"""
from register_timer_metadata import field

ACCESS = ("Halted physical core; the checked PMU adapter uses current Debug EL2, "
          "not stopped CPSR.M. EL0/EL1 access remains unknown without fresh "
          "HDCR.TPM/TPMCR/HPMN and HSTR.T9 permission evidence. Observation never "
          "enables, clears or resets counters, changes filters or writes PMSELR. ")
BITS = [field(f"Event{n}", n, 1, f"Event-counter {n} bit; four physical event counters, not five.") for n in range(4)]
BITS += [field("Cycle", 31, 1, "Cycle-counter bit; separate from PMCR.N event-counter capacity.")]
PMU_METADATA = {
    "pmcr": {"description": "PMU implementation and counting controls. PMCR.N from EL0/EL1 can reflect HDCR.HPMN; only current Debug EL2 proves the physical count. PMCR.E=0 does not mean absent.",
             "fields": [field("E", 0, 1, "Counter enable; a read never starts counting."),
                        field("P", 1, 1, "Event reset command; reads as zero, never written by the reader.", access="wo"),
                        field("C", 2, 1, "Cycle reset command; reads as zero, never written by the reader.", access="wo"),
                        field("D", 3, 1, "Cycle divider: zero counts each clock, one counts each 64 clocks."),
                        field("X", 4, 1, "Event export enable; observing counters does not enable trace export."),
                        field("DP", 5, 1, "Disable cycle counting when event counting is prohibited."),
                        field("LC", 6, 1, "Select bit 31 or bit 63 overflow detection; PMCCNTR storage remains 64 bits for either value."),
                        field("N", 11, 5, "Four event counters at EL2. Lower-EL reads can return the Guest partition; excludes the cycle counter.", access="ro"),
                        field("IDCODE", 16, 8, "R52 PMU identification code 0x13.", access="ro"),
                        field("IMP", 24, 8, "Arm implementer 0x41.", access="ro")]},
    "pmselr": {"description": "Selected PMU event index. 0-3 select implemented events; 31 makes PMXEVTYPER access PMCCFILTR but does not make PMXEVCNTR a cycle-count register. Indexed reads preserve this selector.",
               "fields": [field("SEL", 0, 5, "Event index 0-3 or cycle-filter selector 31. No selector write during native PMU reads.")]},
    "pmccntr": {"description": "Full 64-bit cycle counter, one genuine MRRC p15,0,Rt,Rt2,c9. Neither PMCR.LC=0 nor a 32-bit MRC alias truncates this native storage view.",
                 "fields": [field("CycleCount", 0, 64, "Modulo-2^64 cycle count; independent reads are not simultaneous or an elapsed-time measurement.")]},
    "pmxevcntr": {"description": "32-bit count selected by PMSELR. Only indices 0-3 are valid; SEL=31 is UNDEFINED on R52, not PMCCNTR. Prefer PMEVCNTRn direct addressing.",
                   "fields": [field("SelectedEventCount", 0, 32, "One event-count sample from the observed PMSELR, without changing selection.")]},
    "pmxevtyper": {"description": "Current selected event type; SEL=31 instead exposes the cycle filter PMCCFILTR. Reading does not choose an event or modify the selector.",
                    "fields": [field("SelectedTypeOrCycleFilter", 0, 32, "Interpret using the fresh PMSELR evidence; cycle filters do not contain an event selector.")]},
    "pmccfiltr": {"description": "Cycle-count filter, addressed directly without PMSELR selection. Counter enable, filter and debug controls govern counting separately from readable storage.",
                  "fields": [field("CycleFilter", 0, 32, "Raw cycle-filter control word; no automatic filter changes.")]},
    "pmuserenr": {"description": "EL0 PMU access enables. These bits alone do not bypass Hyp traps or establish Debug-state permission.",
                  "fields": [field("UserAccessControl", 0, 32, "Raw user-access controls; observing does not grant EL0 access.")]},
}
for name, description in {
    "pmcntenset": "Counter-enable set view; reads observe enabled bits without enabling any counter.",
    "pmcntenclr": "Counter-enable clear view; reads observe enabled bits without disabling any counter.",
    "pmintenset": "Overflow-interrupt enable set view; read does not enable interrupts.",
    "pmintenclr": "Overflow-interrupt enable clear view; read does not disable interrupts.",
    "pmovsr": "Overflow flags (clear-on-write view); read never acknowledges or clears an overflow.",
    "pmovsset": "Overflow flags (set-on-write view); read never sets or clears an overflow.",
}.items():
    PMU_METADATA[name] = {"description": description, "fields": BITS}
for bank in (0, 1):
    PMU_METADATA[f"pmceid{bank}"] = {
        "description": f"Implemented common-event bitmap for event numbers {bank*32}-{bank*32+31}; this describes event support, not counter count or permission.",
        "fields": [field("EventSupport", 0, 32, "Bit k declares support for the corresponding common event; observation does not select or start it.", access="ro")],
    }
for n in range(4):
    PMU_METADATA[f"pmevcntr{n}"] = {"description": f"Physical event counter {n}, complete 32-bit value. Direct MRC addressing preserves PMSELR; current EL2 and fresh PMCR.N establish capacity.",
                                  "fields": [field("EventCount", 0, 32, "Modulo-2^32 event sample; overflow flags are separate and never cleared by this read.")]}
    PMU_METADATA[f"pmevtyper{n}"] = {"description": f"Physical event type/filter {n}, direct MRC addressing. Reader never changes event selection or filters.",
                                   "fields": [field("EventTypeAndFilter", 0, 32, "Raw event type and filtering controls; event-code support is described separately by PMCEID0/1.")]}
for metadata in PMU_METADATA.values():
    metadata["access_condition"] = ACCESS
