//! A catalogue declaration is separate from the route used by a value request.
use super::{Context, Reader};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Planned,
    Started,
    Responded,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Acquisition {
    Catalogue,
    CapabilityProbe,
    MpuRegions,
    SelectorBank,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ByteOrder {
    Little,
    Big,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Route {
    GdbRegister {
        /// Known only after DebugTUI selected this connection and no opaque CLI command followed.
        endpoint: Option<String>,
        configured_endpoint: String,
        name: String,
        index: usize,
    },
    GdbMemory {
        endpoint: Option<String>,
        configured_endpoint: String,
        address: String,
        bits: u16,
        byte_order: ByteOrder,
    },
    TclRegister {
        endpoint: String,
        /// Name actually placed in the selection request; not a physical identity observation.
        target: String,
        operation: String,
    },
    TclMemory {
        endpoint: String,
        target: String,
        channel: String,
        configuration_source: String,
        address: String,
        bits: u16,
        bus_width: u16,
        count: u16,
        byte_order: ByteOrder,
        atomic: bool,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Access {
    /// Fresh physical evidence bound to this request, never an implementation fact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timer: Option<super::timer::Evidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pmu: Option<super::pmu::Evidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gic: Option<super::gic::Evidence>,
    pub route: Route,
    pub phase: Phase,
    /// Exact MI command or Tcl transaction submitted to the transport, without framing.
    pub command: String,
    pub context: Context,
    /// Route resolution time until dispatch, then the actual write-start time.
    pub timestamp_ms: u64,
    /// Host response completion, in the same session-relative monotonic clock.
    /// A request interval bounds transport execution, not the hardware clock.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_ms: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Derivation {
    pub source: String,
    pub offset: u16,
    pub bits: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Provenance {
    pub acquisition: Acquisition,
    /// Declaration, not evidence that this reader executed.
    pub catalogue_reader: Reader,
    pub access: Option<Access>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<Derivation>,
}
impl Provenance {
    pub fn declared(reader: &Reader) -> Self {
        Self {
            acquisition: Acquisition::Catalogue,
            catalogue_reader: reader.clone(),
            access: None,
            aliases: vec![],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", content = "provenance", rename_all = "snake_case")]
pub enum RetainedOrigin {
    Known(Box<Provenance>),
    Unknown,
}

#[cfg(test)]
mod tests;
