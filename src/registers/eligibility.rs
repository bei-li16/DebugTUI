//! Saved capability conditions are distinct from access permission and read results.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Configuration,
    Observation,
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConditionEvidence {
    pub register: String,
    pub fact: String,
    pub min: u64,
    pub max: Option<u64>,
    pub value: Option<u64>,
    pub configured_value: Option<u64>,
    pub source: Source,
    pub observation: Option<capabilities::Fact>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Observation {
    pub id: String,
    pub state: State,
    pub reason: Reason,
    pub detail: String,
    pub raw: Option<RawValue>,
    pub source: String,
    pub timestamp_ms: u64,
    /// Successful request proof is retained with the decision, even without a data read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<provenance::Provenance>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProbeBasis {
    pub context: Context,
    pub thread: String,
    pub identity: Option<capabilities::Identity>,
    pub observations: Vec<Observation>,
    pub notes: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub implementation: Implementation,
    pub context: Context,
    pub catalogue_cpu: String,
    pub catalogue_architecture: String,
    pub catalogue_source: Option<String>,
    pub dependencies: Vec<String>,
    pub conditions: Vec<ConditionEvidence>,
    pub detail: String,
    pub probe: Option<ProbeBasis>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "evidence", rename_all = "snake_case")]
pub enum Retained {
    Known(Box<Evidence>),
    Unknown,
}

impl Catalogue {
    /// A validated alias cannot bypass a parent's optional or access conditions.
    pub fn read_dependencies<'a>(
        &'a self,
        register: &'a Register,
    ) -> Result<Vec<&'a Register>, String> {
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        let mut current = register;
        loop {
            if !seen.insert(&current.id) {
                return Err("Alias dependency cycle; capability remains unknown".into());
            }
            result.push(current);
            let Reader::Alias { source, .. } = &current.reader else {
                break;
            };
            current = self
                .register(source)
                .ok_or_else(|| format!("Unknown alias dependency {source}"))?;
        }
        Ok(result)
    }
    pub fn implementation(
        &self,
        register: &Register,
        facts: &BTreeMap<String, u64>,
    ) -> (Implementation, String) {
        let dependencies = match self.read_dependencies(register) {
            Ok(dependencies) => dependencies,
            Err(error) => return (Implementation::Unknown, error),
        };
        let mut missing = BTreeSet::new();
        let mut conditional = false;
        for dependency in dependencies {
            for condition in &dependency.conditions {
                conditional = true;
                match facts.get(&condition.fact) {
                    Some(&value)
                        if value < condition.min
                            || condition.max.is_some_and(|max| value > max) =>
                    {
                        return (
                            Implementation::No,
                            format!(
                                "{}: {}={value} excludes this register",
                                dependency.id, condition.fact
                            ),
                        );
                    }
                    Some(_) => {}
                    None => {
                        missing.insert(condition.fact.as_str());
                    }
                }
            }
        }
        if !missing.is_empty() {
            (
                Implementation::Unknown,
                format!(
                    "Unknown capability: {}",
                    missing.into_iter().collect::<Vec<_>>().join(", ")
                ),
            )
        } else if !conditional {
            (
                Implementation::Unknown,
                "Target identity not yet verified".into(),
            )
        } else {
            (
                Implementation::Yes,
                "Capability conditions satisfied".into(),
            )
        }
    }
    pub fn read_policy(&self, register: &Register) -> (bool, bool) {
        self.read_dependencies(register)
            .map(|dependencies| {
                (
                    dependencies.iter().all(|r| r.access.readable()),
                    dependencies.iter().any(|r| r.read_side_effect),
                )
            })
            .unwrap_or((false, false))
    }
    pub fn automatic_read(&self, register: &Register, facts: &BTreeMap<String, u64>) -> bool {
        let (readable, side_effect) = self.read_policy(register);
        let implementation = self.implementation(register, facts).0;
        readable
            && !side_effect
            && (implementation == Implementation::Yes
                || (implementation == Implementation::Unknown
                    && self.read_dependencies(register).is_ok_and(|dependencies| {
                        dependencies.iter().all(|r| r.conditions.is_empty())
                    })))
    }
    pub fn eligibility(
        &self,
        register: &Register,
        declared: &BTreeMap<String, u64>,
        probe: Option<&capabilities::Probe>,
        context: &Context,
    ) -> Evidence {
        let probe = probe.filter(|p| p.context == *context && context.frame == 0);
        let facts = probe
            .map(|p| p.effective(declared))
            .unwrap_or_else(|| declared.clone());
        let (implementation, detail) = self.implementation(register, &facts);
        let dependencies = self.read_dependencies(register).unwrap_or_default();
        let conditions: Vec<_> = dependencies
            .iter()
            .flat_map(|r| {
                r.conditions.iter().map(|condition| {
                    let observation = probe.and_then(|p| p.facts.get(&condition.fact)).cloned();
                    let configured_value = declared.get(&condition.fact).copied();
                    let source = if observation.is_some() {
                        Source::Observation
                    } else if configured_value.is_some() {
                        Source::Configuration
                    } else {
                        Source::Unknown
                    };
                    ConditionEvidence {
                        register: r.id.clone(),
                        fact: condition.fact.clone(),
                        min: condition.min,
                        max: condition.max,
                        value: facts.get(&condition.fact).copied(),
                        configured_value,
                        source,
                        observation,
                    }
                })
            })
            .collect();
        let probe = probe
            .filter(|_| !conditions.is_empty())
            .map(|p| ProbeBasis {
                context: p.context.clone(),
                thread: p.thread.clone(),
                identity: p.identity.clone(),
                notes: p.notes.clone(),
                observations: p
                    .samples
                    .iter()
                    .filter(|sample| {
                        sample.context == *context
                            && sample.owner.as_deref() == Some(&format!("core:{}", context.core))
                    })
                    .map(|sample| Observation {
                        id: sample.id.clone(),
                        state: sample.state,
                        reason: sample.reason,
                        detail: sample.detail.clone(),
                        raw: if sample.state == State::Valid {
                            sample.value.clone()
                        } else {
                            None
                        },
                        source: sample.source.clone(),
                        timestamp_ms: sample.timestamp_ms,
                        provenance: (sample.state == State::Valid)
                            .then(|| sample.provenance.clone())
                            .flatten(),
                    })
                    .collect(),
            });
        Evidence {
            implementation,
            context: context.clone(),
            catalogue_cpu: self.cpu.clone(),
            catalogue_architecture: self.architecture.clone(),
            catalogue_source: None,
            dependencies: dependencies.iter().map(|r| r.id.clone()).collect(),
            conditions,
            detail,
            probe,
        }
    }
}
impl Evidence {
    pub fn with_catalogue_source(mut self, source: impl Into<String>) -> Self {
        self.catalogue_source = Some(source.into());
        self
    }
    pub fn lines(&self, label: &str) -> Vec<String> {
        let mut lines = vec![
            format!("{label}: {:?} · {}", self.implementation, self.detail),
            format!(
                "Capability basis: core={} frame={} stop={} session={}",
                self.context.core,
                self.context.frame,
                self.context.generation,
                self.context.session
            ),
            format!("Reader dependencies: {}", self.dependencies.join(" → ")),
            format!(
                "Decision catalogue: {} / {} via {}",
                self.catalogue_cpu,
                self.catalogue_architecture,
                self.catalogue_source.as_deref().unwrap_or("Unknown")
            ),
        ];
        for condition in &self.conditions {
            lines.push(format!(
                "{}: {}={} required {}..{} · {:?}",
                condition.register,
                condition.fact,
                condition
                    .value
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "Unknown".into()),
                condition.min,
                condition
                    .max
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "unbounded".into()),
                condition.source
            ));
            if condition.source == Source::Observation {
                lines.push(format!(
                    "Declared value: {}",
                    condition
                        .configured_value
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "none".into())
                ));
            }
            if let Some(observation) = &condition.observation {
                lines.push(format!(
                    "Observed {} via {}: {}",
                    observation.register, observation.source, observation.detail
                ));
            }
        }
        if let Some(probe) = &self.probe {
            lines.push(format!(
                "Probe thread {} · CPU {}",
                probe.thread,
                probe
                    .identity
                    .as_ref()
                    .and_then(|i| i.model.as_deref())
                    .unwrap_or("Unknown")
            ));
            for observation in &probe.observations {
                lines.push(format!(
                    "Probe {}: {:?} raw={} via {} at {} ms · {:?} {}",
                    observation.id,
                    observation.state,
                    observation
                        .raw
                        .as_ref()
                        .map(|v| v.hex.as_str())
                        .unwrap_or("Unknown"),
                    observation.source,
                    observation.timestamp_ms,
                    observation.reason,
                    observation.detail
                ));
                if let Some(access) = observation
                    .provenance
                    .as_ref()
                    .and_then(|p| p.access.as_ref())
                {
                    lines.push(format!(
                        "Probe request: {:?} · {:?} · {} · {}..{} ms · core={} frame={} stop={} session={}",
                        access.route, access.phase, access.command, access.timestamp_ms,
                        access.completed_ms.map(|n| n.to_string()).unwrap_or_else(|| "Unknown".into()),
                        access.context.core, access.context.frame, access.context.generation,
                        access.context.session
                    ));
                    if let Some(gic) = &access.gic {
                        lines.push(format!(
                            "GIC capacity proof: {:?} {:?} MIDR={} EDSCR={} DSPSR={} DLR={} ID_PFR1={}",
                            gic.view, gic.read_method, gic.midr.hex, gic.dscr.hex,
                            gic.dspsr.hex, gic.dlr.hex, gic.id_pfr1.hex
                        ));
                        lines.push(format!(
                            "GIC controls: ICC_HSRE={} ICC_SRE={} ICC_CTLR={} ICH_VTR={} HCR={} ICH_HCR={} HSTR={}",
                            gic.icc_hsre.hex, gic.icc_sre.hex, gic.icc_ctlr.hex, gic.ich_vtr.hex,
                            gic.hcr.hex, gic.ich_hcr.hex, gic.hstr.hex
                        ));
                    }
                }
            }
            lines.extend(probe.notes.iter().cloned());
        }
        lines
    }
}

#[cfg(test)]
mod tests;
