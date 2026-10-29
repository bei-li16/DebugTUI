//! Declarative discovery: selected definitions and current field evidence only.
//! No chip addresses, transport commands, writes or implicit bootstrap exemptions.
use super::*;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Discovery {
    pub registers: Vec<String>,
    pub rules: Vec<Rule>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub fact: String,
    pub requires: Vec<policy::FieldCondition>,
    pub description: String,
}

impl Discovery {
    pub fn is_empty(&self) -> bool {
        self.registers.is_empty() && self.rules.is_empty()
    }
    pub(super) fn merge(&mut self, other: Self) -> Result<(), String> {
        for id in other.registers {
            if !self.registers.contains(&id) {
                self.registers.push(id);
            }
        }
        for rule in other.rules {
            if self.rules.iter().any(|r| r.fact == rule.fact) {
                return Err(format!("Ambiguous inherited discovery fact {}", rule.fact));
            }
            self.rules.push(rule);
        }
        Ok(())
    }
    pub(super) fn validate(&self, catalogue: &Catalogue) -> Result<(), String> {
        if self.registers.len() > 128 || self.rules.len() > 64 {
            return Err("Discovery exceeds 128 reads / 64 rules".into());
        }
        let mut ids = BTreeSet::new();
        for id in &self.registers {
            if !ids.insert(id) || catalogue.register(id).is_none() {
                return Err(format!("Duplicate or missing discovery register {id}"));
            }
        }
        let mut facts = BTreeSet::new();
        for rule in &self.rules {
            if rule.fact.is_empty()
                || rule.fact.len() > 128
                || !rule
                    .fact
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                || rule.fact.starts_with(policy::FIELD_FACT_PREFIX)
                || m_profile::FACT_KEYS.contains(&rule.fact.as_str())
                || !facts.insert(&rule.fact)
                || rule.requires.is_empty()
                || rule.requires.len() > 64
                || rule.description.trim().is_empty()
            {
                return Err(format!("Invalid discovery rule {}", rule.fact));
            }
            for condition in &rule.requires {
                let source = catalogue
                    .register(&condition.reg)
                    .ok_or_else(|| format!("Missing discovery source {}", condition.reg))?;
                let field = source
                    .fields
                    .iter()
                    .find(|f| f.name == condition.field)
                    .ok_or_else(|| {
                        format!("Missing discovery field {}", condition.description())
                    })?;
                let width: u16 = field.segments.iter().map(|s| s.width).sum();
                if width > 64
                    || RawValue::from_integer(condition.value.into(), width).is_err()
                    || source.scope != Scope::Core
                    || catalogue.read_policy(source) != (true, false)
                {
                    return Err(format!(
                        "Unsafe discovery source {}",
                        condition.description()
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) fn decode(
        &self,
        fields: &BTreeMap<String, (u64, &Sample)>,
    ) -> BTreeMap<String, capabilities::Fact> {
        let values = fields.iter().map(|(k, (v, _))| (k.clone(), *v)).collect();
        self.rules
            .iter()
            .filter(|rule| {
                rule.requires
                    .iter()
                    .all(|c| c.evaluate(&values) == Some(true))
            })
            .map(|rule| {
                let ids: BTreeSet<_> = rule.requires.iter().map(|c| c.reg.as_str()).collect();
                (
                    rule.fact.clone(),
                    capabilities::Fact {
                        value: 1,
                        register: ids.into_iter().collect::<Vec<_>>().join(", "),
                        source: "selected catalogue discovery; current physical field observations"
                            .into(),
                        detail: rule.description.clone(),
                    },
                )
            })
            .collect()
    }
}

/// Probe is an automatic read, never permission to bypass a chip override.
pub fn denial(
    catalogue: &Catalogue,
    register: &Register,
    facts: &BTreeMap<String, u64>,
) -> Option<(Reason, String)> {
    if let Some(denial) = catalogue.declared_read_denial(register) {
        return Some(denial);
    }
    if register.scope != Scope::Core {
        return Some((
            Reason::AccessRestricted,
            "Core capability probe requires a core-owned definition; no IO sent".into(),
        ));
    }
    if !catalogue.read_policy(register).0 {
        return Some((
            Reason::WriteOnly,
            "Probe skipped write-only definition; no IO sent".into(),
        ));
    }
    if catalogue.read_policy(register).1 {
        return Some((
            Reason::AccessRestricted,
            "Probe skipped read side effect; explicit manual read required".into(),
        ));
    }
    if !catalogue.automatic_read(register, facts) {
        let (implementation, detail) = catalogue.implementation(register, facts);
        return Some((
            if implementation == Implementation::No {
                Reason::HardwareNotImplemented
            } else {
                Reason::Unknown
            },
            detail,
        ));
    }
    None
}
