//! Runtime route gate shared by production reads and UI demand scheduling.
use super::*;
use crate::config::MemoryAccess;

pub fn denial(
    catalogue: &Catalogue,
    register: &Register,
    config: &Config,
    channels: &[MemoryAccess],
    core: &str,
    owner: Option<&str>,
) -> Option<(Reason, String)> {
    let halt = |message: &str| Some((Reason::AccessRestricted, format!("NeedHalt: {message}")));
    let dependencies = match catalogue.read_dependencies(register) {
        Ok(d) => d,
        Err(error) => return Some((Reason::ReaderUnsupported, error)),
    };
    if dependencies
        .iter()
        .any(|r| r.access_rule.need_halt != Some(false))
    {
        return halt("register or alias dependency requires a stopped target");
    }
    let root = dependencies.last().unwrap();
    let binding = match &root.reader {
        Reader::CorePrivate { .. } => {
            if catalogue.cpu == "cortex-m7"
                && catalogue
                    .register("scb.ccsidr")
                    .is_some_and(|r| r.reader == root.reader)
            {
                return halt(
                    "indexed cache reads require the protected stopped selector transaction",
                );
            }
            core_private::binding(config, channels, core)
        }
        Reader::Mmio {
            component,
            require_owner_mapping,
            ..
        } => {
            if config.mmio_probe || stm::component(component) {
                return halt("this component requires a fresh stopped identity/control proof");
            }
            config.component(component, owner, *require_owner_mapping)
        }
        _ => return halt("this reader requires stopped GDB/core execution state"),
    };
    let binding = match binding {
        Ok(binding) => binding,
        Err(error) => return Some((Reason::ReaderUnsupported, error)),
    };
    if binding.channel.is_empty() {
        return halt("GDB memory fallback has no running-state capability");
    }
    match channels.iter().find(|c| c.id == binding.channel) {
        Some(channel)
            if (channel.cores.is_empty() || channel.cores.iter().any(|c| c == core))
                && channel.while_running =>
        {
            None
        }
        Some(channel) if channel.cores.is_empty() || channel.cores.iter().any(|c| c == core) => {
            halt("the selected memory channel does not permit running reads")
        }
        _ => Some((
            Reason::ReaderUnsupported,
            "Memory channel is unavailable for this core".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn running_memory_view_requires_a_completed_actual_ap_interval_and_current_owner() {
        let mut sample:Sample=serde_json::from_value(json!({"id":"scb.cpuid","state":"valid","implementation":"yes","reason":"unknown","detail":"",
            "value":{"bits":32,"hex":"0x410fc241"},"owner":"core:m4","context":{"session":1,"generation":2,"core":"m4","frame":0},
            "timestamp_ms":23,"source":"ppb","view":"physical_core"})).unwrap();
        assert!(sample.runtime_matches(true));
        assert!(!sample.runtime_matches(false));
        sample.view = SampleView::RunningMemory;
        assert!(!sample.runtime_matches(false));
        sample.provenance=Some(serde_json::from_value(json!({"acquisition":"catalogue","catalogue_reader":{"kind":"core_private","address":0xe000ed00u32},
            "access":{"route":{"kind":"tcl_memory","endpoint":"localhost:6666","target":"cpu4","channel":"ppb4","configuration_source":"test","address":"0xe000ed00","bits":32,"bus_width":32,"count":1,"byte_order":"little","atomic":true},
            "phase":"responded","command":"cpu4 read_memory 0xe000ed00 32 1","context":sample.context,"timestamp_ms":20,"completed_ms":23}})).unwrap());
        assert!(sample.runtime_matches(false));
        assert!(!sample.runtime_matches(true));
        let mut frame = sample.context.clone();
        frame.frame = 3;
        assert!(sample.applies(&frame, Some("core:m4")));
        frame.core = "m7".into();
        assert!(!sample.applies(&frame, Some("core:m7")));
        for bad in ["planned", "incomplete", "backwards", "context", "gdb"] {
            let mut invalid = sample.clone();
            let a = invalid
                .provenance
                .as_mut()
                .unwrap()
                .access
                .as_mut()
                .unwrap();
            match bad {
                "planned" => a.phase = provenance::Phase::Planned,
                "incomplete" => a.completed_ms = None,
                "backwards" => a.completed_ms = Some(19),
                "context" => a.context.generation += 1,
                "gdb" => {
                    a.route = provenance::Route::GdbMemory {
                        endpoint: Some("localhost:3333".into()),
                        configured_endpoint: "localhost:3333".into(),
                        address: "0xe000ed00".into(),
                        bits: 32,
                        byte_order: provenance::ByteOrder::Little,
                    }
                }
                _ => unreachable!(),
            }
            assert!(!invalid.runtime_matches(false), "{bad}");
        }
        let legacy:Sample=serde_json::from_value(json!({"id":"r0","state":"valid","implementation":"unknown","reason":"unknown","detail":"","timestamp_ms":0,"source":"legacy","context":sample.context})).unwrap();
        assert!(legacy.runtime_matches(true));
        assert!(!legacy.runtime_matches(false));
    }
}
