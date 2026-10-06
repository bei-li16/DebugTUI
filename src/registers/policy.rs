//! Finite field comparisons. Configuration facts never impersonate observations.
use super::*;
use provenance::{Phase, Route};

pub const FIELD_FACT_PREFIX: &str = "register.";
pub fn field_key(reg: &str, field: &str) -> String {
    format!(
        "{FIELD_FACT_PREFIX}{}:{reg}:{}:{field}",
        reg.len(),
        field.len()
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compare {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}
impl Compare {
    pub fn matches(self, actual: u64, expected: u64) -> bool {
        match self {
            Self::Eq => actual == expected,
            Self::Ne => actual != expected,
            Self::Lt => actual < expected,
            Self::Le => actual <= expected,
            Self::Gt => actual > expected,
            Self::Ge => actual >= expected,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldCondition {
    pub reg: String,
    pub field: String,
    pub op: Compare,
    pub value: u64,
}
impl FieldCondition {
    pub fn key(&self) -> String {
        field_key(&self.reg, &self.field)
    }
    pub fn evaluate(&self, facts: &BTreeMap<String, u64>) -> Option<bool> {
        facts
            .get(&self.key())
            .map(|n| self.op.matches(*n, self.value))
    }
    pub fn description(&self) -> String {
        format!("{}.{} {:?} {}", self.reg, self.field, self.op, self.value)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AccessRule {
    /// Omitted keeps the conservative stopped-only policy. False is limited to memory readers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub need_halt: Option<bool>,
    /// Must be supplied by current backend Debug-state evidence, never saved CPSR/configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_el: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub need_enable: Option<FieldCondition>,
}
impl AccessRule {
    pub fn is_empty(&self) -> bool {
        self.need_halt.is_none() && self.min_el.is_none() && self.need_enable.is_none()
    }
}

impl Catalogue {
    pub(super) fn validate_policy(&self) -> Result<(), String> {
        for register in &self.registers {
            if register.access_rule.min_el.is_some_and(|el| el > 3) {
                return Err(format!("Invalid minimum Debug EL for {}", register.id));
            }
            let dependencies = self.read_dependencies(register)?;
            let root = dependencies.last().unwrap();
            if register.access_rule.need_halt == Some(false)
                && !matches!(
                    root.reader,
                    Reader::Mmio { .. } | Reader::CorePrivate { .. }
                )
            {
                return Err(format!(
                    "{} cannot disable NeedHalt for a non-memory reader",
                    register.id
                ));
            }
            for condition in register
                .present_if
                .iter()
                .chain(register.access_rule.need_enable.iter())
            {
                let source = self.register(&condition.reg).ok_or_else(|| {
                    format!(
                        "{} condition references missing register {}",
                        register.id, condition.reg
                    )
                })?;
                let field = source
                    .fields
                    .iter()
                    .find(|f| f.name == condition.field)
                    .ok_or_else(|| {
                        format!(
                            "{} condition references missing field {}.{}",
                            register.id, condition.reg, condition.field
                        )
                    })?;
                let width: u16 = field.segments.iter().map(|s| s.width).sum();
                let (readable, side_effect) = self.read_policy(source);
                if width > 64
                    || RawValue::from_integer(u128::from(condition.value), width).is_err()
                    || source.scope != register.scope
                    || side_effect
                    || !readable
                {
                    return Err(format!(
                        "Invalid field condition for {}: {}",
                        register.id,
                        condition.description()
                    ));
                }
            }
        }
        // Include alias, presence and enable dependencies in one bounded cycle check.
        fn visit<'a>(
            catalogue: &'a Catalogue,
            id: &'a str,
            active: &mut BTreeSet<&'a str>,
            done: &mut BTreeMap<&'a str, usize>,
            depth: usize,
        ) -> Result<usize, String> {
            if let Some(height) = done.get(id) {
                return Ok(*height);
            }
            if depth > 64 || !active.insert(id) {
                return Err(format!("Register policy dependency cycle/depth at {id}"));
            }
            let register = catalogue.register(id).unwrap();
            let mut height = 1;
            if let Reader::Alias { source, .. } = &register.reader {
                height = height.max(1 + visit(catalogue, source, active, done, depth + 1)?);
            }
            for c in register
                .present_if
                .iter()
                .chain(register.access_rule.need_enable.iter())
            {
                height = height.max(1 + visit(catalogue, &c.reg, active, done, depth + 1)?);
            }
            if height > 64 {
                return Err(format!("Register policy dependency cycle/depth at {id}"));
            }
            active.remove(id);
            done.insert(id, height);
            Ok(height)
        }
        let mut done = BTreeMap::new();
        for register in &self.registers {
            visit(self, &register.id, &mut BTreeSet::new(), &mut done, 0)?;
        }
        Ok(())
    }

    /// Only successful, current physical samples with an actual response can supply field facts.
    pub fn observation_facts(
        &self,
        declared: &BTreeMap<String, u64>,
        probe: Option<&capabilities::Probe>,
        samples: &[Sample],
        context: &Context,
    ) -> BTreeMap<String, u64> {
        self.observation_facts_for_owners(declared, probe, samples, context, &Topology::default())
    }
    pub fn observation_facts_for_owners(
        &self,
        declared: &BTreeMap<String, u64>,
        probe: Option<&capabilities::Probe>,
        samples: &[Sample],
        context: &Context,
        topology: &Topology,
    ) -> BTreeMap<String, u64> {
        let probe = probe.filter(|p| p.context == *context && context.frame == 0);
        let mut facts = probe
            .map(|p| p.effective(declared))
            .unwrap_or_else(|| declared.clone());
        facts.retain(|key, _| !key.starts_with(FIELD_FACT_PREFIX));
        if super::m_profile::adapted_cpu(&self.cpu) {
            facts.retain(|key, _| !super::m_profile::FACT_KEYS.contains(&key.as_str()));
            facts.extend(super::m_profile::current_facts(
                self, probe, samples, context,
            ));
        }
        facts.extend(
            self.field_observations(probe, samples, context, topology)
                .into_iter()
                .map(|(key, (value, _))| (key, value)),
        );
        facts
    }
    pub(super) fn field_observations<'a>(
        &self,
        probe: Option<&'a capabilities::Probe>,
        samples: &'a [Sample],
        context: &Context,
        topology: &Topology,
    ) -> BTreeMap<String, (u64, &'a Sample)> {
        let probe = probe.filter(|p| p.context == *context && context.frame == 0);
        let mut latest: BTreeMap<&str, &Sample> = BTreeMap::new();
        for sample in probe
            .into_iter()
            .flat_map(|p| p.samples.iter())
            .chain(samples)
        {
            if sample.context == *context
                && latest
                    .get(sample.id.as_str())
                    .is_none_or(|previous| sample.timestamp_ms >= previous.timestamp_ms)
            {
                latest.insert(&sample.id, sample);
            }
        }
        let mut fields = BTreeMap::new();
        for sample in latest.values().copied() {
            let Some(register) = self.register(&sample.id) else {
                continue;
            };
            let expected_owner = topology.owner(register.scope, &context.core);
            if sample.state != State::Valid
                || sample.context != *context
                || context.frame != 0
                || sample.view != SampleView::PhysicalCore
                || expected_owner.is_none()
                || sample.owner != expected_owner
            {
                continue;
            }
            let Some(raw) = sample.value.as_ref().filter(|v| v.bits == register.bits) else {
                continue;
            };
            let Some(provenance) = sample
                .provenance
                .as_ref()
                .filter(|p| p.catalogue_reader == register.reader)
            else {
                continue;
            };
            let Some(access) = provenance.access.as_ref().filter(|a| {
                a.context == *context
                    && a.phase == Phase::Responded
                    && a.completed_ms.is_some_and(|end| end >= a.timestamp_ms)
            }) else {
                continue;
            };
            let root = self
                .read_dependencies(register)
                .ok()
                .and_then(|d| d.last().copied());
            let route_matches = match (root.map(|r| &r.reader), &access.route) {
                (
                    Some(Reader::CorePrivate { address }),
                    Route::TclMemory {
                        address: actual,
                        bits,
                        ..
                    }
                    | Route::GdbMemory {
                        address: actual,
                        bits,
                        ..
                    },
                ) => actual == &format!("0x{address:x}") && *bits == root.unwrap().bits,
                (Some(Reader::Mmio { .. }), Route::TclMemory { .. } | Route::GdbMemory { .. }) => {
                    true
                }
                (Some(Reader::Gdb { name }), Route::GdbRegister { name: actual, .. }) => {
                    name == actual
                }
                (
                    Some(
                        Reader::Cp15 { .. }
                        | Reader::Cp15_64 { .. }
                        | Reader::Backend { .. }
                        | Reader::Banked { .. }
                        | Reader::Vfp { .. },
                    ),
                    Route::TclRegister { .. },
                ) => true,
                _ => false,
            };
            if !route_matches {
                continue;
            }
            if !super::m_profile::field_allowed(self, &sample.id, &latest, context) {
                continue;
            }
            for field in &register.fields {
                if let Ok(value) = field.extract(raw).and_then(|v| v.integer())
                    && let Ok(value) = u64::try_from(value)
                {
                    fields.insert(field_key(&register.id, &field.name), (value, sample));
                }
            }
        }
        fields
    }

    pub fn has_presence_rule(&self, register: &Register) -> bool {
        self.read_dependencies(register)
            .is_ok_and(|deps| deps.iter().any(|r| r.present_if.is_some()))
    }
    pub fn access_denial(
        &self,
        register: &Register,
        facts: &BTreeMap<String, u64>,
        stopped: bool,
        current_debug_el: Option<u8>,
    ) -> Option<(Reason, String)> {
        let dependencies = match self.read_dependencies(register) {
            Ok(d) => d,
            Err(e) => return Some((Reason::Unknown, e)),
        };
        if dependencies.iter().any(|r| r.scope == Scope::Unknown) {
            return Some((
                Reason::Unknown,
                "Register owner is unknown; no read sent".into(),
            ));
        }
        if !stopped
            && dependencies
                .iter()
                .any(|r| r.access_rule.need_halt != Some(false))
        {
            return Some((
                Reason::AccessRestricted,
                "NeedHalt: reader requires a stopped target".into(),
            ));
        }
        for r in dependencies {
            if let Some(min_el) = r.access_rule.min_el {
                match current_debug_el {
                    Some(el) if el >= min_el => {}
                    Some(el) => {
                        return Some((
                            Reason::AccessRestricted,
                            format!("NeedEl({min_el}): current Debug EL is {el}"),
                        ));
                    }
                    None => {
                        return Some((
                            Reason::Unknown,
                            format!(
                                "{}: current Debug EL is unproven; saved CPSR/configuration cannot authorize this read",
                                r.id
                            ),
                        ));
                    }
                }
            }
            if let Some(condition) = &r.access_rule.need_enable {
                match condition.evaluate(facts) {
                    Some(true) => {}
                    Some(false) => {
                        return Some((
                            Reason::FeatureDisabled,
                            format!("NeedEnable: {}", condition.description()),
                        ));
                    }
                    None => {
                        return Some((
                            Reason::Unknown,
                            format!(
                                "NeedEnable is unknown: {}; read its source explicitly or probe capabilities",
                                condition.description()
                            ),
                        ));
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests;
