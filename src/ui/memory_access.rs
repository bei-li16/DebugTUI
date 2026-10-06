//! Validate a completed read against the requested route before displaying it.
use super::*;
use crate::registers::{
    Context, RawValue,
    provenance::{Access, ByteOrder, Phase, Route},
};

#[derive(Clone)]
pub(super) struct Receipt {
    pub access: Access,
    pub state: String,
}
#[derive(Clone, Copy)]
pub(super) struct Expected<'a> {
    pub context: &'a Context,
    pub epoch: u64,
    pub channel: &'a str,
    pub address: &'a str,
    pub bits: u16,
    pub little: bool,
    pub dump: bool,
}
impl Receipt {
    pub fn caption(&self) -> String {
        let route = match &self.access.route {
            Route::GdbMemory {
                endpoint,
                configured_endpoint,
                ..
            } => format!(
                "GDB {} @ {} · source=GDB{}",
                self.access.context.core,
                endpoint.as_deref().unwrap_or("unknown"),
                if endpoint.is_none() {
                    format!(" (configured {configured_endpoint})")
                } else {
                    String::new()
                }
            ),
            Route::TclMemory {
                channel,
                target,
                endpoint,
                configuration_source,
                ..
            } => format!("{channel} → {target} @ {endpoint} · source={configuration_source}"),
            _ => "unavailable memory origin".into(),
        };
        format!("{route} · {} · non-atomic", self.state)
    }
}
impl App {
    pub(super) fn memory_route_fingerprint(&self, channel: &str) -> String {
        json!([
            self.project.debug.chip,
            self.snapshot
                .core
                .as_ref()
                .map(|c| json!([c.index, c.name, c.endpoint])),
            self.project.target.mode,
            self.memory_gdb_configured_endpoint(),
            self.project.memory_access_source,
            channel,
            self.project.memory_access.iter().find(|c| c.id == channel)
        ])
        .to_string()
    }
    fn memory_gdb_configured_endpoint(&self) -> &str {
        self.snapshot
            .core
            .as_ref()
            .map(|core| core.endpoint.as_str())
            .unwrap_or(&self.project.target.endpoint)
    }
    pub(super) fn memory_receipt(
        &self,
        result: &Value,
        expected: &Expected<'_>,
    ) -> Result<Receipt, String> {
        let Expected {
            context,
            epoch,
            channel,
            address,
            bits,
            little,
            dump,
        } = *expected;
        if !(if dump {
            bits > 0 && bits <= 32768 && bits.is_multiple_of(8)
        } else {
            matches!(bits, 8 | 16 | 32 | 64)
        }) {
            return Err("Unsupported memory receipt width".into());
        }
        let access: Access = serde_json::from_value(result["access"].clone())
            .map_err(|_| "Memory response has no complete access receipt")?;
        let state = result["state"]
            .as_str()
            .ok_or("Memory response has no target state")?;
        if access.context != *context
            || serde_json::from_value::<Context>(result["context"].clone())
                .ok()
                .as_ref()
                != Some(context)
            || result["selection_epoch"].as_u64() != Some(epoch)
            || result["channel"].as_str() != Some(channel)
            || access.phase != Phase::Responded
            || !access
                .completed_ms
                .is_some_and(|end| end >= access.timestamp_ms)
            || !matches!(state, "STOPPED" | "RUNNING")
            || result["atomic"] != false
            || state != self.snapshot.state
        {
            return Err(
                "Memory response has an expired or incomplete route/context receipt".into(),
            );
        }
        let order = if little {
            ByteOrder::Little
        } else {
            ByteOrder::Big
        };
        let valid = match &access.route {
            Route::GdbMemory {
                endpoint,
                configured_endpoint,
                address: actual,
                bits: width,
                byte_order,
            } if channel.is_empty() => {
                let command = if dump {
                    format!(
                        "-data-read-memory-bytes {} {}",
                        crate::mi::quote(address),
                        bits / 8
                    )
                } else {
                    format!("-data-read-memory-bytes {address} {}", bits / 8)
                };
                state == "STOPPED"
                    && *configured_endpoint == self.memory_gdb_configured_endpoint()
                    && endpoint
                        .as_ref()
                        .is_none_or(|endpoint| endpoint == configured_endpoint)
                    && actual == address
                    && *width == bits
                    && *byte_order == order
                    && access.command == command
            }
            Route::TclMemory {
                channel: actual_channel,
                target,
                endpoint,
                configuration_source,
                address: actual,
                bits: width,
                bus_width,
                count,
                byte_order,
                atomic,
            } if !channel.is_empty() => self
                .project
                .memory_access
                .iter()
                .find(|c| c.id == channel)
                .is_some_and(|configured| {
                    let bus = if dump { 8 } else { bits.min(32) };
                    actual_channel == channel
                        && target == &configured.target
                        && endpoint == &configured.tcl_endpoint
                        && configuration_source == &self.project.memory_access_source
                        && actual == address
                        && *width == bits
                        && *bus_width == bus
                        && *count == bits / bus
                        && *byte_order == order
                        && !atomic
                        && (configured.cores.is_empty() || configured.cores.contains(&context.core))
                        && (state == "STOPPED" || configured.while_running)
                        && access.command
                            == format!(
                                "{} read_memory {address} {bus} {}",
                                crate::live_watch::word(target),
                                bits / bus
                            )
                }),
            _ => false,
        };
        if !valid {
            return Err("Memory response receipt does not match the requested target, endpoint, source or operation".into());
        }
        if !dump {
            let base = u64::from_str_radix(address.trim_start_matches("0x"), 16)
                .map_err(|_| "Invalid scalar address")?;
            let value = result["value"]
                .as_u64()
                .ok_or("Memory response has no scalar value")?;
            let raw: RawValue = serde_json::from_value(result["raw"].clone())
                .map_err(|_| "Memory response has no exact raw value")?;
            if result["address"].as_u64() != Some(base)
                || result["bits"].as_u64() != Some(u64::from(bits))
                || raw != RawValue::from_integer(u128::from(value), bits)?
            {
                return Err(
                    "Memory scalar response has inconsistent address, width or raw value".into(),
                );
            }
        }
        Ok(Receipt {
            access,
            state: state.into(),
        })
    }
}

#[cfg(test)]
pub(super) fn scalar_fixture(app: &App, request: &Request, value: u64) -> Value {
    let channel = request.params["channel"].as_str().unwrap_or("");
    let address = request.params["address"].as_u64().unwrap();
    let bits = request.params["bits"].as_u64().unwrap() as u16;
    let little = request.params["little_endian"].as_bool().unwrap();
    let mut result = fixture(app, channel, &format!("0x{address:x}"), bits, little, false);
    result["address"] = json!(address);
    result["bits"] = json!(bits);
    result["value"] = json!(value);
    result["raw"] = json!(RawValue::from_integer(u128::from(value), bits).unwrap());
    result
}
#[cfg(test)]
pub(super) fn fixture(
    app: &App,
    channel: &str,
    address: &str,
    bits: u16,
    little: bool,
    dump: bool,
) -> Value {
    let byte_order = if little {
        ByteOrder::Little
    } else {
        ByteOrder::Big
    };
    let (route, command) = if channel.is_empty() {
        (
            Route::GdbMemory {
                endpoint: None,
                configured_endpoint: app.memory_gdb_configured_endpoint().into(),
                address: address.into(),
                bits,
                byte_order,
            },
            if dump {
                format!(
                    "-data-read-memory-bytes {} {}",
                    crate::mi::quote(address),
                    bits / 8
                )
            } else {
                format!("-data-read-memory-bytes {address} {}", bits / 8)
            },
        )
    } else {
        let c = app
            .project
            .memory_access
            .iter()
            .find(|c| c.id == channel)
            .unwrap();
        let bus = if dump { 8 } else { bits.min(32) };
        (
            Route::TclMemory {
                endpoint: c.tcl_endpoint.clone(),
                target: c.target.clone(),
                channel: channel.into(),
                configuration_source: app.project.memory_access_source.clone(),
                address: address.into(),
                bits,
                bus_width: bus,
                count: bits / bus,
                byte_order,
                atomic: false,
            },
            format!(
                "{} read_memory {address} {bus} {}",
                crate::live_watch::word(&c.target),
                bits / bus
            ),
        )
    };
    json!({"context":app.register_context(),"selection_epoch":app.snapshot.memory_selection_epoch,
        "channel":channel,"state":app.snapshot.state,"atomic":false,
        "access":{"route":route,"command":command,"context":app.register_context(),"phase":"responded","timestamp_ms":1,"completed_ms":2}})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_wide_scalar_receipt_rejects_wrong_source_layout_and_raw_value() {
        let mut app = App::new(Project::default(), false);
        app.snapshot.state = "STOPPED".into();
        app.project.memory_access_source = "profile:fixture".into();
        app.project.memory_access.push(crate::config::MemoryAccess {
            id: "bus".into(),
            target: "soc.bus".into(),
            tcl_endpoint: "localhost:6666".into(),
            cores: vec!["default".into()],
            ..Default::default()
        });
        let request = Request::new(
            1,
            "memory_read",
            json!({"channel":"bus","address":0x100000008u64,"bits":64,"little_endian":false}),
        );
        let result = scalar_fixture(&app, &request, 0xfedcba9876543210u64);
        let context = app.register_context();
        let expected = Expected {
            context: &context,
            epoch: 0,
            channel: "bus",
            address: "0x100000008",
            bits: 64,
            little: false,
            dump: false,
        };
        let receipt = app.memory_receipt(&result, &expected).unwrap();
        assert!(
            receipt
                .caption()
                .contains("soc.bus @ localhost:6666 · source=profile:fixture")
        );
        assert_eq!(result["raw"]["hex"], "0xfedcba9876543210");
        for scenario in [
            "source", "target", "endpoint", "count", "width", "endian", "raw", "context", "channel",
        ] {
            let mut bad = result.clone();
            match scenario {
                "source" => bad["access"]["route"]["configuration_source"] = json!("other-profile"),
                "target" => bad["access"]["route"]["target"] = json!("soc.other"),
                "endpoint" => bad["access"]["route"]["endpoint"] = json!("other:6666"),
                "count" => bad["access"]["route"]["count"] = json!(1),
                "width" => bad["access"]["route"]["bus_width"] = json!(64),
                "endian" => bad["access"]["route"]["byte_order"] = json!("little"),
                "raw" => bad["raw"] = json!(RawValue::from_integer(0, 64).unwrap()),
                "context" => bad["access"]["context"]["core"] = json!("other-core"),
                "channel" => bad["access"]["route"]["channel"] = json!("other-channel"),
                _ => unreachable!(),
            }
            assert!(app.memory_receipt(&bad, &expected).is_err(), "{scenario}");
        }
    }
}
