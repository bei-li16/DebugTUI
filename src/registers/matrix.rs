//! Read-only capability inventory. Configuration is a plan; successful samples
//! describe one context and never grant permission for a future instruction.
use super::provenance::Phase;
use super::*;
use crate::config::MemoryAccess;
use crate::session::state;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Environment {
    pub registers: Config,
    pub selected_core: Option<String>,
    pub gdb_endpoint: String,
    pub observed_gdb_endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub gdb_core_endpoints: BTreeMap<String, String>,
    pub channels: Vec<MemoryAccess>,
    pub channels_source: String,
    pub target_state: String,
    pub access_fault: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub transport: String,
    pub operation: String,
    pub endpoint: Option<String>,
    pub target: Option<String>,
    pub protocol: Option<String>,
    pub address: Option<String>,
    pub channel: Option<String>,
    pub configuration_source: Option<String>,
    pub memory: Option<MemoryPlan>,
    pub available: bool,
    pub detail: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryPlan {
    pub byte_order: provenance::ByteOrder,
    pub bus_width: Option<u16>,
    pub count: Option<u16>,
    pub atomic: bool,
}
impl Plan {
    fn missing(detail: &str) -> Self {
        Self {
            transport: "unknown".into(),
            operation: String::new(),
            endpoint: None,
            target: None,
            protocol: None,
            address: None,
            channel: None,
            configuration_source: None,
            memory: None,
            available: false,
            detail: detail.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    Unobserved,
    ObservedValue,
    UnprovenValue,
    ConditionsExcluded,
    NotImplemented,
    WriteOnly,
    UnknownOwner,
    RouteUnavailable,
    ReaderUnsupported,
    AccessRestricted,
    FeatureDisabled,
    Error,
    Stale,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Row {
    pub id: String,
    pub category: Vec<String>,
    pub bits: u16,
    pub scope: Scope,
    pub owner: Option<String>,
    pub dependencies: Vec<String>,
    pub state_conditions: Vec<String>,
    pub plan: Plan,
    pub readable: bool,
    pub manual_only: bool,
    pub automatic_eligible: bool,
    pub implementation: Implementation,
    pub implementation_detail: String,
    pub support: Support,
    /// Index into observations, which preserves latest and retained provenance.
    pub observation: Option<usize>,
    pub observation_context_current: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Category {
    pub group: String,
    pub entries: usize,
    pub widths: Vec<u16>,
    pub observed_values: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlannedClass {
    pub id: String,
    pub groups: Vec<String>,
    pub entries: usize,
    pub configured_routes: usize,
    pub observed_values: usize,
    pub hardware_support: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u32,
    pub source: String,
    pub context: Context,
    /// Complete declarations, stored once. An absent group is not hardware No.
    pub catalogue: Option<Catalogue>,
    pub environment: Environment,
    pub probe: Option<capabilities::Probe>,
    pub probe_current: bool,
    pub effective_facts: BTreeMap<String, u64>,
    pub fact_source: String,
    pub observations: Vec<Sample>,
    pub owner_generations: BTreeMap<String, u64>,
    pub rows: Vec<Row>,
    pub categories: Vec<Category>,
    pub planned_classes: Vec<PlannedClass>,
    pub meaning: String,
}
impl Report {
    pub fn refresh(&mut self) {
        self.rows.clear();
        self.categories.clear();
        self.planned_classes.clear();
        let config = &self.environment.registers;
        let probe = self.probe.as_ref().filter(|p| {
            self.environment.target_state == state::STOPPED
                && p.context == self.context
                && self.context.frame == 0
        });
        let facts = probe
            .map(|p| p.effective(&config.facts))
            .unwrap_or_else(|| config.facts.clone());
        self.probe_current = probe.is_some();
        self.fact_source = if self.probe_current {
            "configuration_and_current_observation"
        } else {
            "configuration"
        }
        .into();
        let Some(catalogue) = &self.catalogue else {
            self.effective_facts = facts;
            return;
        };
        let facts = catalogue.observation_facts_for_owners(
            &config.facts,
            probe,
            if self.environment.target_state == state::STOPPED {
                &self.observations
            } else {
                &[]
            },
            &self.context,
            &config.topology,
        );
        self.effective_facts = facts.clone();
        for register in &catalogue.registers {
            let dependencies = catalogue
                .read_dependencies(register)
                .expect("Validated catalogue");
            let root = dependencies.last().expect("Register dependency");
            let owner = config.topology.owner(register.scope, &self.context.core);
            let plan = plan(root, &self.environment, &self.context, owner.as_deref());
            let (readable, manual_only) = catalogue.read_policy(register);
            let (implementation, implementation_detail) =
                catalogue.implementation(register, &facts);
            let observation = self.observations.iter().position(|s| s.id == register.id);
            let sample = observation.map(|i| &self.observations[i]);
            let current = sample.is_some_and(|s| {
                self.environment.target_state == state::STOPPED
                    && s.applies_at(&self.context, owner.as_deref(), &self.owner_generations)
            });
            let support = if owner.is_none() {
                Support::UnknownOwner
            } else if implementation == Implementation::No {
                Support::ConditionsExcluded
            } else if !readable {
                Support::WriteOnly
            } else if let Some(sample) = sample {
                if !current {
                    Support::Stale
                } else {
                    match sample.state {
                        State::Valid => {
                            if sample
                                .value
                                .as_ref()
                                .is_some_and(|v| v.bits == register.bits)
                                && sample.value_provenance().is_some_and(|p| {
                                    p.catalogue_reader == register.reader
                                        && p.access.as_ref().is_some_and(|a| {
                                            a.phase == Phase::Responded
                                                && a.context == sample.context
                                                && a.completed_ms
                                                    .is_some_and(|end| end >= a.timestamp_ms)
                                        })
                                })
                            {
                                Support::ObservedValue
                            } else {
                                Support::UnprovenValue
                            }
                        }
                        State::Stale => Support::Stale,
                        State::Error => Support::Error,
                        State::NotRead => Support::Unobserved,
                        _ => match sample.reason {
                            Reason::HardwareNotImplemented => Support::NotImplemented,
                            Reason::ReaderUnsupported => Support::ReaderUnsupported,
                            Reason::AccessRestricted => Support::AccessRestricted,
                            Reason::FeatureDisabled => Support::FeatureDisabled,
                            Reason::WriteOnly => Support::WriteOnly,
                            Reason::TransportError => Support::Error,
                            Reason::Unknown => Support::Unknown,
                        },
                    }
                }
            } else if !plan.available {
                Support::RouteUnavailable
            } else {
                Support::Unobserved
            };
            let mut category = vec![register.group.clone()];
            while let Some(parent) = catalogue
                .groups
                .iter()
                .find(|g| g.id == *category.last().unwrap())
                .and_then(|g| g.parent.clone())
            {
                category.push(parent);
            }
            category.reverse();
            let mut state_conditions = vec!["STOPPED; this context and owner only".into()];
            if plan.transport == "tcl" {
                state_conditions
                    .push("Physical frame 0; current target/thread checked by reader".into());
            }
            if matches!(&root.reader, Reader::Mmio { component, .. } if config.mmio_probe || super::stm::component(component))
            {
                state_conditions
                    .push("Fresh owner-scoped MMIO identity/capacity proof required".into());
            }
            state_conditions.extend(
                dependencies
                    .iter()
                    .filter(|r| !r.access_condition.is_empty())
                    .map(|r| format!("{}: {}", r.id, r.access_condition)),
            );
            for dependency in &dependencies {
                if let Some(condition) = &dependency.present_if {
                    state_conditions.push(format!(
                        "{} present_if: {}",
                        dependency.id,
                        condition.description()
                    ));
                }
                if let Some(condition) = &dependency.access_rule.need_enable {
                    state_conditions.push(format!(
                        "{} NeedEnable: {}",
                        dependency.id,
                        condition.description()
                    ));
                }
                if let Some(el) = dependency.access_rule.min_el {
                    state_conditions.push(format!(
                        "{}: current Debug EL >= {el}; saved CPSR cannot authorize",
                        dependency.id
                    ));
                }
            }
            self.rows.push(Row {
                id: register.id.clone(),
                category,
                bits: register.bits,
                scope: register.scope,
                owner,
                dependencies: dependencies.iter().map(|r| r.id.clone()).collect(),
                state_conditions,
                plan,
                readable,
                manual_only,
                automatic_eligible: catalogue.automatic_read(register, &facts)
                    && catalogue
                        .access_denial(
                            register,
                            &facts,
                            self.environment.target_state == state::STOPPED,
                            None,
                        )
                        .is_none(),
                implementation,
                implementation_detail,
                support,
                observation,
                observation_context_current: current,
            });
        }
        for group in &catalogue.groups {
            let rows: Vec<_> = self
                .rows
                .iter()
                .filter(|r| r.category.contains(&group.id))
                .collect();
            self.categories.push(Category {
                group: group.id.clone(),
                entries: rows.len(),
                widths: rows
                    .iter()
                    .map(|r| r.bits)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                observed_values: rows
                    .iter()
                    .filter(|r| r.support == Support::ObservedValue)
                    .count(),
            });
        }
        if catalogue.architecture == "armv8-r-aarch32" {
            for (id, groups) in [
                ("core", vec!["core"]),
                ("banked", vec!["irq", "fiq", "und", "abt", "svc", "hyp"]),
                ("vfp", vec!["simd"]),
                ("cp15_system", vec!["id", "control", "exceptions", "virt"]),
                ("mpu", vec!["mpu_el1", "mpu_el2"]),
                ("timer", vec!["timer"]),
                ("pmu", vec!["pmu"]),
                ("gic_physical", vec!["gic_icc"]),
                ("gic_virtual", vec!["gic_ich", "gic_icv"]),
                ("gic_mmio", vec!["gicd", "gicr"]),
                ("debug", vec!["debug"]),
                ("stm", vec!["stm"]),
            ] {
                let rows: Vec<_> = self
                    .rows
                    .iter()
                    .filter(|r| groups.iter().any(|g| r.category.iter().any(|c| c == g)))
                    .collect();
                self.planned_classes.push(PlannedClass {
                    id: id.into(), groups: groups.into_iter().map(String::from).collect(), entries: rows.len(),
                    configured_routes: rows.iter().filter(|r| r.plan.available).count(),
                    observed_values: rows.iter().filter(|r| r.support == Support::ObservedValue).count(),
                    hardware_support: "unverified".into(),
                    detail: if rows.is_empty() { "No matching catalogue entries; presence, mapping and backend remain unknown" }
                    else { "Declared group coverage; plans and cached values do not prove physical hardware support" }.into(),
                });
            }
        }
    }
}

fn plan(
    register: &Register,
    environment: &Environment,
    context: &Context,
    owner: Option<&str>,
) -> Plan {
    let config = &environment.registers;
    let gdb = |operation: String| {
        Plan {
        transport: "gdb".into(), operation, endpoint: Some(environment.gdb_endpoint.clone()),
        target: Some(context.core.clone()), protocol: None, address: None, channel: None,
        configuration_source: Some("project".into()), memory: None, available: true,
        detail: "Configured route; connection, target description and access remain unverified until a read".into(),
    }
    };
    let tcl = |operation: String, protocol: Option<&str>| {
        let target = config.targets.get(&context.core).cloned();
        let available = !config.tcl_endpoint.is_empty()
            && target.is_some()
            && environment.access_fault.is_none();
        Plan {
            transport: "tcl".into(),
            operation,
            endpoint: Some(config.tcl_endpoint.clone()),
            target,
            protocol: protocol.map(String::from),
            address: None,
            channel: None,
            configuration_source: Some("registers".into()),
            memory: None,
            available,
            detail: environment.access_fault.clone().unwrap_or_else(|| {
                if available {
                    "Configured route and required protocol; no protocol command was executed"
                        .into()
                } else {
                    "Register endpoint or selected-core target is missing".into()
                }
            }),
        }
    };
    for (command, route, protocol) in [
        (
            &config.gic_command,
            gic::route(&register.reader),
            gic::PROTOCOL,
        ),
        (
            &config.pmu_command,
            pmu::route(&register.reader),
            pmu::PROTOCOL,
        ),
        (
            &config.timer_command,
            timer::route(&register.reader),
            timer::PROTOCOL,
        ),
    ] {
        if !command.is_empty()
            && let Some((name, bits)) = route
        {
            if register.bits != bits {
                return Plan::missing("Adapter requires the native register width");
            }
            return tcl(format!("{command} {name}"), Some(protocol));
        }
    }
    match &register.reader {
        Reader::Gdb { name } => gdb(format!("register {name}")),
        Reader::Cp15 {
            cp,
            op1,
            crn,
            crm,
            op2,
        } => {
            if config.cp15_command.is_empty() {
                Plan::missing("No CP15 MRC command configured")
            } else if config.cp15_command == super::r52_core::COMMAND {
                match super::r52_core::Request::from_reader(&register.reader) {
                    Some(request) if register.bits == 32 && register.scope == Scope::Core => tcl(
                        format!("{} {}", config.cp15_command, request.name),
                        Some(super::r52_core::PROTOCOL),
                    ),
                    _ => Plan::missing(
                        "No bounded R52 identity/control/MPU route for this definition",
                    ),
                }
            } else {
                tcl(
                    format!("{} {cp} {op1} {crn} {crm} {op2}", config.cp15_command),
                    None,
                )
            }
        }
        Reader::Cp15_64 { cp, op1, crm } => {
            if config.cp15_64_command.is_empty() {
                gdb(format!("register {}", register.id))
            } else {
                tcl(
                    format!("{} {cp} {op1} {crm}", config.cp15_64_command),
                    Some(OPENOCD_ADAPTER_PROTOCOL),
                )
            }
        }
        Reader::Backend { name } => tcl(format!("get_reg {name}"), None),
        Reader::Banked { name } => {
            if config.banked_command.is_empty() {
                Plan::missing("No banked adapter configured")
            } else {
                tcl(
                    format!("{} {name}", config.banked_command),
                    Some(banked::PROTOCOL),
                )
            }
        }
        Reader::Vfp { name } => {
            if config.vfp_command.is_empty() {
                Plan::missing("No VFP adapter configured")
            } else {
                tcl(
                    format!("{} {name}", config.vfp_command),
                    Some(vfp::PROTOCOL),
                )
            }
        }
        Reader::Alias { .. } => Plan::missing("Alias root is unresolved"),
        Reader::CorePrivate { address } => {
            let binding = match core_private::binding(config, &environment.channels, &context.core)
            {
                Ok(binding) => binding,
                Err(error) => return Plan::missing(&error),
            };
            if binding.channel.is_empty()
                && let Err(error) = core_private::gdb_endpoint(
                    &environment.gdb_endpoint,
                    environment.observed_gdb_endpoint.as_deref(),
                    &environment.gdb_core_endpoints,
                    &context.core,
                )
            {
                return Plan::missing(&error);
            }
            let mut memory = register.clone();
            memory.reader = Reader::Mmio {
                component: "ppb".into(),
                offset: *address,
                require_owner_mapping: true,
            };
            let mut result = plan(&memory, environment, context, owner);
            if result.available {
                result.detail =
                    "CorePrivate PPB through this core's explicit route; no shared fallback".into();
            }
            result
        }
        Reader::Mmio {
            component,
            offset,
            require_owner_mapping,
        } => {
            let binding = match config.component(component, owner, *require_owner_mapping) {
                Ok(binding) => binding,
                Err(error) => return Plan::missing(&error),
            };
            let Some(address) = binding.base.checked_add(*offset) else {
                return Plan::missing("Component address overflow");
            };
            if !matches!(register.bits, 8 | 16 | 32 | 64)
                || !address.is_multiple_of(u64::from(register.bits / 8))
                || address.checked_add(u64::from(register.bits / 8)).is_none()
            {
                return Plan::missing("Unsupported or unaligned memory width/address");
            }
            let mut result = if binding.channel.is_empty() {
                gdb(format!("memory 0x{address:x}"))
            } else {
                let Some(channel) = environment
                    .channels
                    .iter()
                    .find(|a| a.id == binding.channel)
                    .filter(|a| {
                        a.cores.is_empty()
                            || environment
                                .selected_core
                                .as_ref()
                                .is_some_and(|core| a.cores.contains(core))
                    })
                else {
                    return Plan::missing("Memory channel is unavailable for this core");
                };
                Plan { transport: "tcl_memory".into(), operation: "read_memory".into(),
                    endpoint: Some(channel.tcl_endpoint.clone()), target: Some(channel.target.clone()),
                    protocol: None, address: None, channel: Some(channel.id.clone()),
                    configuration_source: Some(environment.channels_source.clone()), memory: None, available: true,
                    detail: "Configured AP/owner mapping; hardware identity and access remain unverified".into(),
                }
            };
            result.address = Some(format!("0x{address:x}"));
            result.channel = Some(binding.channel.clone());
            result.memory = Some(MemoryPlan {
                byte_order: if binding.little_endian {
                    provenance::ByteOrder::Little
                } else {
                    provenance::ByteOrder::Big
                },
                bus_width: (!binding.channel.is_empty()).then_some(register.bits.min(32)),
                count: (!binding.channel.is_empty())
                    .then_some(register.bits / register.bits.min(32)),
                atomic: false,
            });
            result
        }
    }
}

#[cfg(test)]
mod tests;
