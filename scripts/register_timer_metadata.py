"""R52 Generic Timer descriptions; no target access or writer declarations.

Encodings: R52 TRM 100026_0104_01_en Table 11-1, pp.380-381.
Access: DDI0568A.c Table E1-1. The unchanged timer fields are defined in
DDI0406C.b B4.1.21-35 and B8.1. No Armv8-A-only extensions are imported.
"""

COMMON_ACCESS = (
    "Halted physical core. Normal-execution EL rules follow; Debug-state access "
    "requires separate backend evidence, including Hyp debug authorization/EDSCR.HDD; "
    "the stopped CPSR alone does not establish debugger permission. "
)
PHYSICAL_ACCESS = COMMON_ACCESS + (
    "EL2: accessible. EL1: CNTHCTL.PL1PCEN controls physical timer access. "
    "EL0: also requires CNTKCTL.PL0PTEN; a permission failure does not prove absence."
)
VIRTUAL_ACCESS = COMMON_ACCESS + (
    "EL1/EL2: virtual timer view. EL0: CNTKCTL.PL0VTEN controls access; "
    "a virtual view is not the physical counter or Hyp timer."
)
HYP_ACCESS = COMMON_ACCESS + "EL2/Hyp only; EL1/EL0 cannot use this register."


def field(name, offset, width, description, enums=(), access=None):
    return {"name": name, "segments": [(offset, width)], "description": description,
            "enums": enums, "access": access}


CTL_FIELDS = [
    field("ENABLE", 0, 1, "Timer output enable; reading never enables the timer.",
          [(0, "Disabled"), (1, "Enabled")]),
    field("IMASK", 1, 1, "Masks the timer output; does not change ISTATUS.",
          [(0, "Unmasked"), (1, "Masked")]),
    field("ISTATUS", 2, 1,
          "Raw timer condition bit; meaningful only when ENABLE=1. UNKNOWN when disabled; independent of IMASK.",
          access="ro"),
]
TVAL_FIELDS = [field("TimerValue", 0, 32,
                     "Signed two's-complement (CompareValue - counter)[31:0], in ticks; read value is UNKNOWN when disabled.")]
EVENT_FIELDS = [
    field("EVNTEN", 2, 1, "Event-stream generation enable; unrelated to timer output ENABLE.",
          [(0, "Disabled"), (1, "Enabled")]),
    field("EVNTDIR", 3, 1, "Selected counter-bit transition for event generation.",
          [(0, "Rising"), (1, "Falling")]),
    field("EVNTI", 4, 4, "Selected counter bit 0-15 for the event stream; not a timer or PMU count."),
]

TIMER_METADATA = {
    "cntfrq": {
        "description": "Software-provided counter frequency in Hz; not a measured clock. Zero or an unset value does not prove timer absence.",
        "access_condition": COMMON_ACCESS + "EL1/EL2 read; writes only at EL2. EL0 read requires CNTKCTL.PL0PCTEN or PL0VCTEN.",
        "fields": [field("ClockFrequency", 0, 32, "Firmware's counter-frequency value in Hz; validate independently before converting ticks to time.")],
    },
    "cntkctl": {
        "description": "Kernel controls for EL0 counter/timer access and the virtual-counter event stream; not FPU or Timer enable.",
        "access_condition": COMMON_ACCESS + "EL1/EL2 access; unavailable from EL0.",
        "fields": [
            field("PL0PCTEN", 0, 1, "EL0 physical-count access; with PL0VCTEN also controls EL0 CNTFRQ reads."),
            field("PL0VCTEN", 1, 1, "EL0 virtual-count access; with PL0PCTEN also controls EL0 CNTFRQ reads."),
            *EVENT_FIELDS,
            field("PL0VTEN", 8, 1, "EL0 access to CNTV_CTL, CNTV_TVAL and CNTV_CVAL."),
            field("PL0PTEN", 9, 1, "EL0 access to CNTP_CTL, CNTP_TVAL and CNTP_CVAL; higher-level restrictions still apply."),
        ],
    },
    "cnthctl": {
        "description": "Hyp controls for lower-EL physical-counter/timer access and the physical-counter event stream.",
        "access_condition": HYP_ACCESS,
        "fields": [
            field("PL1PCTEN", 0, 1, "Lower-EL physical-count access (PL1 is EL1); EL0 CNTKCTL restrictions still apply."),
            field("PL1PCEN", 1, 1, "Lower-EL physical-timer access (PL1 is EL1); EL0 CNTKCTL restrictions still apply."),
            *EVENT_FIELDS,
        ],
    },
    "cntpct": {
        "description": "64-bit physical counter, one genuine MRRC sample; independent reads at different times need not match.",
        "access_condition": COMMON_ACCESS + "EL2 read; lower-EL access is controlled by CNTHCTL.PL1PCTEN and, at EL0, CNTKCTL.PL0PCTEN.",
        "fields": [field("PhysicalCount", 0, 64, "Complete physical count in ticks; one sample, not unrelated low/high MRC reads.", access="ro")],
    },
    "cntvct": {
        "description": "64-bit virtual count (physical count minus CNTVOFF modulo 2^64); belongs to the current core's virtual view.",
        "access_condition": COMMON_ACCESS + "EL1/EL2 read; EL0 requires CNTKCTL.PL0VCTEN. CNTVOFF cannot be inferred from differently timed samples.",
        "fields": [field("VirtualCount", 0, 64, "Complete virtual count in ticks; separate physical and virtual samples are not simultaneous.", access="ro")],
    },
    "cntvoff": {
        "description": "64-bit Hyp virtual-counter offset; reading does not change Guest time or counter state.",
        "access_condition": HYP_ACCESS,
        "fields": [field("VirtualOffset", 0, 64, "Offset subtracted from the physical count modulo 2^64; EL2 owns this value.")],
    },
}
for prefix, view, access in [("cntp", "EL1 physical", PHYSICAL_ACCESS),
                             ("cntv", "EL1 virtual", VIRTUAL_ACCESS),
                             ("cnthp", "Hyp physical", HYP_ACCESS)]:
    TIMER_METADATA[f"{prefix}_ctl"] = {
        "description": f"{view} timer control and raw condition status. ISTATUS is UNKNOWN when ENABLE=0; IMASK is not an implementation flag.",
        "access_condition": access, "fields": CTL_FIELDS,
    }
    TIMER_METADATA[f"{prefix}_tval"] = {
        "description": f"{view} signed 32-bit timer difference in ticks; disabled reads are architecturally UNKNOWN. This is not the 64-bit counter or a stable interval sample.",
        "access_condition": access, "fields": TVAL_FIELDS,
    }
    TIMER_METADATA[f"{prefix}_cval"] = {
        "description": f"{view} full 64-bit comparator value, read with one MRRC; viewing does not program or enable the timer.",
        "access_condition": access,
        "fields": [field("CompareValue", 0, 64, "Complete comparator storage; independent read time differs from counter/TVAL samples.")],
    }
