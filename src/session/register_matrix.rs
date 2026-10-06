//! Inventory reads only the worker's cached catalogue, configuration and evidence.
use super::*;
use crate::registers::matrix::{Environment, Report};
use std::collections::BTreeMap;

impl Engine {
    pub(super) fn register_matrix(&self) -> Result<Json, String> {
        let configured = self.register_catalogue.as_ref().map_err(Clone::clone)?;
        let (catalogue, source) = configured
            .as_ref()
            .map(|(c, s)| (Some(c.clone()), s.clone()))
            .unwrap_or((None, "gdb".into()));
        let mut registers = self.project.registers.clone();
        if registers.topology.chip.is_empty() {
            registers.topology.chip = self.project.debug.chip.clone();
        }
        let mut report = Report {
            schema_version: 1, catalogue, source, context: self.register_context(),
            environment: Environment {
                registers, selected_core: self.project.preference_core.clone(), gdb_endpoint: self.project.target.endpoint.clone(),
                observed_gdb_endpoint: self.connected_gdb_endpoint.clone(),
                gdb_core_endpoints: self.project.cores.iter().map(|c| (c.name.clone(), c.endpoint.clone())).collect(),
                channels: self.project.memory_access.clone(),
                channels_source: self.project.memory_access_source.clone(),
                target_state: self.snapshot.state.clone(), access_fault: self.register_access_fault.clone(),
            },
            probe: self.snapshot.register_probe.clone(),
            probe_current: false, effective_facts: BTreeMap::new(), fact_source: String::new(),
            observations: self.snapshot.register_samples.clone(),
            owner_generations: BTreeMap::new(), rows: vec![], categories: vec![], planned_classes: vec![],
            meaning: "Configured plans and declared widths do not prove target support. Observed values describe one accepted request/context, not future permission or board validation. No target I/O is performed.".into(),
        };
        report.refresh();
        serde_json::to_value(report).map_err(|e| e.to_string())
    }
}
