//! Shared owners use the producing core's route and an owner lifetime, never a peer stop number.
use super::*;
use crate::registers::{Sample, State, Topology};

pub(super) struct SharedSample {
    epoch: u64,
    sample: Sample,
    last_valid: Option<Sample>,
}

fn shared(sample: &Sample) -> bool {
    sample
        .owner
        .as_deref()
        .is_some_and(|owner| owner.starts_with("cluster:") || owner.starts_with("chip:"))
}
impl Coordinator {
    pub(super) fn filter_register_matrix(
        &self,
        core: usize,
        result: &mut Json,
    ) -> Result<(), String> {
        let mut report = serde_json::from_value::<crate::registers::matrix::Report>(result.clone())
            .map_err(|error| format!("Invalid register matrix: {error}"))?;
        self.filter_shared_samples(core, &mut report.observations);
        if let Some(probe) = &mut report.probe {
            self.filter_shared_samples(core, &mut probe.samples);
            probe.decode();
        }
        report.owner_generations = self.shared_epochs.clone();
        report.refresh();
        *result = json!(report);
        Ok(())
    }

    fn register_topology(&self) -> Topology {
        let mut topology = self.project.registers.topology.clone();
        if topology.chip.is_empty() {
            topology.chip = self.project.debug.chip.clone();
        }
        topology
    }
    pub(super) fn invalidate_shared_owners(&mut self, producer: usize) {
        if !self.multi() {
            return;
        }
        let owners = self
            .register_topology()
            .affected_shared_owners(&self.engines[producer].name);
        if owners.is_empty() {
            return;
        }
        for owner in &owners {
            *self.shared_epochs.entry(owner.clone()).or_default() += 1;
        }
        for engine in &mut self.engines {
            for sample in &mut engine.snapshot.register_samples {
                if sample
                    .owner
                    .as_ref()
                    .is_some_and(|owner| owners.contains(owner))
                {
                    sample.stale();
                }
            }
            if let Some(probe) = &mut engine.snapshot.register_probe {
                for sample in &mut probe.samples {
                    if sample
                        .owner
                        .as_ref()
                        .is_some_and(|owner| owners.contains(owner))
                    {
                        sample.stale();
                    }
                }
                probe.decode();
            }
            Self::shared_legacy_projection(&mut engine.snapshot);
            if !engine.exited && !engine.unresponsive {
                let id = self.next_id;
                self.next_id += 1;
                // A local FIFO boundary also covers an in-flight read which has not stored yet.
                let _ = engine.handle.send(Request::new(
                    id,
                    method::REGISTER_SHARED_INVALIDATE,
                    json!({"owners":owners}),
                ));
            }
        }
    }
    fn last_shared(&self, core: usize, sample: &Sample) -> Option<&SharedSample> {
        self.shared_samples
            .get(&(core, sample.owner.clone()?, sample.id.clone()))
    }
    fn retain_last_shared(&self, core: usize, sample: &mut Sample) {
        let last = self
            .last_shared(core, sample)
            .filter(|last| last.sample.context.session == sample.context.session);
        sample.value = last
            .and_then(|last| last.last_valid.as_ref())
            .and_then(|last| last.value.clone());
        sample.last_value_provenance = None;
        sample.last_value_eligibility = None;
        if let Some(last) = last.and_then(|last| last.last_valid.as_ref()) {
            sample.timestamp_ms = last.timestamp_ms;
            sample.inherit_value_origin(last);
        }
    }
    pub(super) fn filter_shared_snapshot(&self, core: usize, snapshot: &mut Snapshot) {
        if !self.multi() {
            return;
        }
        self.filter_shared_samples(core, &mut snapshot.register_samples);
        if let Some(probe) = &mut snapshot.register_probe {
            self.filter_shared_samples(core, &mut probe.samples);
            probe.decode();
        }
        Self::shared_legacy_projection(snapshot);
    }
    fn filter_shared_samples(&self, core: usize, samples: &mut [Sample]) {
        for sample in samples {
            if !shared(sample) {
                continue;
            }
            let accepted = self.last_shared(core, sample).is_some_and(|last| {
                last.epoch
                    == *self
                        .shared_epochs
                        .get(sample.owner.as_ref().unwrap())
                        .unwrap_or(&0)
                    && last.sample.context == sample.context
                    && last.sample.timestamp_ms == sample.timestamp_ms
                    && last.sample.value == sample.value
            });
            sample.owner_generation = self.last_shared(core, sample).map(|last| last.epoch);
            if sample.state == State::Valid && !accepted {
                sample.stale();
                sample.detail = "Shared sample awaits owner lifetime validation".into();
                self.retain_last_shared(core, sample);
            } else if !accepted {
                // Non-valid worker snapshots may retain a value rejected by the
                // coordinator at an earlier owner boundary. Only the last value
                // accepted on this route is a trustworthy retained origin.
                self.retain_last_shared(core, sample);
            }
        }
    }
    pub(super) fn accept_probe_response(
        &mut self,
        core: usize,
        started: &BTreeMap<String, u64>,
        result: &mut Json,
    ) {
        if !self.multi() {
            return;
        }
        let Ok(mut probe) = serde_json::from_value::<crate::registers::capabilities::Probe>(
            result["probe"].clone(),
        ) else {
            return;
        };
        let mut wrapper = json!({"samples":probe.samples});
        self.accept_shared_response(core, started, &mut wrapper);
        probe.samples = serde_json::from_value(wrapper["samples"].clone())
            .expect("Validated probe sample response");
        probe.decode();
        let configured = serde_json::from_value(result["configured_facts"].clone())
            .unwrap_or_else(|_| self.project.registers.facts.clone());
        result["facts"] = json!(probe.effective(&configured));
        result["probe"] = json!(probe);
        self.engines[core].snapshot.register_probe = Some(probe);
    }
    pub(super) fn accept_shared_response(
        &mut self,
        core: usize,
        started: &BTreeMap<String, u64>,
        result: &mut Json,
    ) {
        if !self.multi() {
            return;
        }
        let Some(values) = result["samples"].as_array_mut() else {
            return;
        };
        for value in values {
            let Ok(mut sample) = serde_json::from_value::<Sample>(value.clone()) else {
                continue;
            };
            if !shared(&sample) {
                continue;
            }
            let owner = sample.owner.clone().unwrap();
            let epoch = *self.shared_epochs.get(&owner).unwrap_or(&0);
            let started_epoch = *started.get(&owner).unwrap_or(&0);
            sample.owner_generation = Some(started_epoch);
            if sample.state == State::Valid && epoch != started_epoch {
                sample.stale();
                sample.detail = "Shared owner changed during read; new value discarded".into();
                self.retain_last_shared(core, &mut sample);
            }
            if epoch == started_epoch {
                let last_valid = if sample.state == State::Valid {
                    Some(sample.clone())
                } else {
                    self.last_shared(core, &sample)
                        .and_then(|last| last.last_valid.clone())
                        .filter(|last| last.context.session == sample.context.session)
                };
                if sample.value.is_none() {
                    sample.value = last_valid.as_ref().and_then(|last| last.value.clone());
                    if let Some(last) = &last_valid {
                        sample.inherit_value_origin(last);
                    }
                }
                self.shared_samples.insert(
                    (core, owner, sample.id.clone()),
                    SharedSample {
                        epoch,
                        sample: sample.clone(),
                        last_valid,
                    },
                );
            }
            let snapshot = &mut self.engines[core].snapshot;
            if let Some(old) = snapshot
                .register_samples
                .iter_mut()
                .find(|old| old.id == sample.id && old.owner == sample.owner)
            {
                *old = sample.clone();
            } else {
                snapshot.register_samples.push(sample.clone());
            }
            *value = json!(sample);
        }
        Self::shared_legacy_projection(&mut self.engines[core].snapshot);
    }
    fn shared_legacy_projection(snapshot: &mut Snapshot) {
        for variable in &mut snapshot.registers {
            if let Some(sample) = snapshot
                .register_samples
                .iter()
                .find(|sample| shared(sample) && sample.source == format!("gdb:{}", variable.name))
            {
                variable.error = sample.state != State::Valid;
                variable.value = sample
                    .value
                    .as_ref()
                    .map(|value| value.hex.clone())
                    .unwrap_or_default();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_timeout_expires_only_affected_shared_owners_and_publishes_the_boundary() {
        let mut project = Project {
            cores: (0..3)
                .map(|i| crate::config::Core {
                    name: format!("core{i}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        project.registers.topology.chip = "board".into();
        project.registers.topology.clusters = [
            ("core0".into(), "A".into()),
            ("core1".into(), "A".into()),
            ("core2".into(), "B".into()),
        ]
        .into();
        let (tx, rx) = std::sync::mpsc::sync_channel(512);
        let mut coordinator = Coordinator::new(project, tx, Arc::new(AtomicBool::new(false)));
        coordinator.active = 0;
        coordinator.shared_epochs = [
            ("cluster:A".into(), 4),
            ("cluster:B".into(), 5),
            ("chip:board".into(), 6),
        ]
        .into();
        for (core, engine) in coordinator.engines.iter_mut().enumerate() {
            engine.snapshot.state = state::STOPPED.into();
            engine.snapshot.register_session = core as u64 + 1;
            engine.snapshot.generation = 2;
            for (id, owner) in [
                ("private", format!("core:core{core}")),
                (
                    "cluster",
                    format!("cluster:{}", if core < 2 { "A" } else { "B" }),
                ),
                ("chip", "chip:board".into()),
            ] {
                engine.snapshot.register_samples.push(serde_json::from_value(json!({
                    "id":id,"state":"valid","implementation":"unknown","reason":"unknown","detail":"",
                    "value":{"bits":32,"hex":"0x01020304"},"owner":owner,
                    "context":{"session":core as u64 + 1,"generation":2,"core":format!("core{core}"),"frame":0},
                    "timestamp_ms":23,"source":"mmio:board","view":"physical_core",
                    "owner_generation":coordinator.shared_epochs.get(&owner)
                })).unwrap());
            }
        }
        coordinator.worker_timeout(1, 999);
        assert!(coordinator.engines[1].unresponsive);
        assert!(
            coordinator.engines[1]
                .handle
                .cancellation
                .load(Ordering::Relaxed)
        );
        assert!(
            !coordinator.engines[0]
                .handle
                .cancellation
                .load(Ordering::Relaxed)
        );
        assert_eq!(coordinator.shared_epochs["cluster:A"], 5);
        assert_eq!(coordinator.shared_epochs["cluster:B"], 5);
        assert_eq!(coordinator.shared_epochs["chip:board"], 7);
        for (core, engine) in coordinator.engines.iter().enumerate() {
            for sample in &engine.snapshot.register_samples {
                let stale = sample.id == "chip" || (sample.id == "cluster" && core < 2);
                assert_eq!(
                    sample.state,
                    if stale { State::Stale } else { State::Valid }
                );
                assert_eq!(sample.value.as_ref().unwrap().hex, "0x01020304");
                assert_eq!(sample.timestamp_ms, 23);
            }
        }
        let Event::Snapshot { snapshot } = rx.try_recv().unwrap() else {
            panic!("the timeout must publish the owner lifetime immediately");
        };
        assert_eq!(snapshot.register_owner_generations["cluster:A"], 5);
        assert_eq!(snapshot.register_owner_generations["cluster:B"], 5);
        assert_eq!(snapshot.register_samples[0].state, State::Valid);
        assert_eq!(snapshot.register_samples[1].state, State::Stale);
    }
}
