//! Sample counts are distinct from actual transport requests (aliases share IO).
use super::*;

#[derive(Debug, Default, Serialize)]
pub struct Summary {
    pub definitions: usize,
    pub states: BTreeMap<String, usize>,
    pub causes: BTreeMap<String, usize>,
    pub acquisitions: BTreeMap<String, usize>,
    pub no_io: usize,
    pub unique_requests: usize,
    pub responded_requests: usize,
}

pub fn summarize(catalogue: &Catalogue, samples: &[Sample]) -> Summary {
    let mut result = Summary {
        definitions: samples.len(),
        ..Summary::default()
    };
    let mut requests = BTreeSet::new();
    let mut responses = BTreeSet::new();
    for sample in samples {
        let state = serde_json::to_value(sample.state)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        *result.states.entry(state).or_default() += 1;
        let register = catalogue.register(&sample.id);
        let policy = register.map(|r| catalogue.read_policy(r));
        let cause = if sample.state == State::Valid {
            "valid"
        } else if sample.state == State::Stale {
            "stale"
        } else if sample.reason == Reason::AccessRestricted {
            "access_restricted"
        } else if sample.reason == Reason::HardwareNotImplemented {
            "hardware_not_implemented"
        } else if policy.is_some_and(|p| !p.0) {
            "write_only"
        } else if sample.reason == Reason::ReaderUnsupported {
            "reader_unsupported"
        } else if sample.reason == Reason::FeatureDisabled {
            "feature_disabled"
        } else if sample.state == State::Error {
            "read_error"
        } else if policy.is_some_and(|p| p.1) {
            "manual_side_effect"
        } else if register.is_some_and(|r| r.access_rule.need_enable.is_some())
            && sample.state == State::Unavailable
        {
            "enable_evidence_missing"
        } else {
            "capability_or_observation_missing"
        };
        *result.causes.entry(cause.into()).or_default() += 1;
        if let Some(provenance) = &sample.provenance {
            let acquisition = serde_json::to_value(provenance.acquisition)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned();
            *result.acquisitions.entry(acquisition).or_default() += 1;
            if let Some(access) = &provenance.access
                && access.phase != provenance::Phase::Planned
            {
                let key = serde_json::to_string(&(
                    &access.context,
                    &access.route,
                    &access.command,
                    access.timestamp_ms,
                ))
                .unwrap();
                requests.insert(key.clone());
                if access.phase == provenance::Phase::Responded {
                    responses.insert(key);
                }
                continue;
            }
        }
        result.no_io += 1;
    }
    result.unique_requests = requests.len();
    result.responded_requests = responses.len();
    result
}
