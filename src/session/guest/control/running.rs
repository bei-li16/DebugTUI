//! GDB all-stop cannot send a z packet during execution. Disable its logical
//! breakpoint, uninsert the owned physical site via OpenOCD, and defer GDB
//! deletion until the next genuine stop. Never delete a shared user site.
use super::*;
use std::net::{SocketAddr, ToSocketAddrs};

#[derive(Clone)]
pub(super) struct Route {
    pub core: String,
    pub endpoint: String,
    pub target: String,
    pub port: u16,
    pub peer: SocketAddr,
}

fn connection_port(text: &str, expected: &str) -> Result<u16, String> {
    let active = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix('*'))
        .collect::<Vec<_>>();
    if active.len() != 1 {
        return Err("Cannot identify one current GDB connection for running cancellation".into());
    }
    let fields = active[0].split_whitespace().collect::<Vec<_>>();
    // GDB appends a human-readable Description column after the endpoint.
    if fields.len() < 3
        || fields[0].parse::<u32>().is_err()
        || !matches!(fields[1], "remote" | "extended-remote")
        || fields[2] != expected
    {
        return Err("Current GDB connection does not match the configured core endpoint".into());
    }
    expected
        .rsplit_once(':')
        .and_then(|(_, p)| p.parse::<u16>().ok())
        .filter(|p| *p != 0)
        .ok_or_else(|| "Running cancellation requires an explicit TCP GDB endpoint".into())
}

fn points(record: &Record) -> Result<Vec<&Value>, String> {
    record
        .data
        .field("BreakpointTable")
        .and_then(|v| v.field("body"))
        .map(Value::items)
        .map(|items| items.iter().map(|v| v.field("bkpt").unwrap_or(v)).collect())
        .ok_or_else(|| "GDB omitted the breakpoint ownership table".into())
}

fn site(value: &Value, pc: u64) -> Result<bool, String> {
    if value.string("enabled") == "n" {
        return Ok(false);
    }
    if let Some(locations) = value.field("locations").filter(|v| !v.items().is_empty()) {
        let mut found = false;
        for location in locations.items() {
            found |= site(location.field("bkpt").unwrap_or(location), pc)?;
        }
        return Ok(found);
    }
    let address = value.string("addr");
    if value.field("pending").is_some() || address == "<PENDING>" {
        return Ok(false);
    }
    literal_address(&address)
        .map(|n| n & !1 == pc & !1)
        .map_err(|_| "A user code breakpoint has no verifiable physical address".into())
}

pub(super) fn ownership(
    record: &Record,
    id: &str,
    pc: u64,
    disabled: bool,
) -> Result<bool, String> {
    let points = points(record)?;
    let own = points
        .iter()
        .find(|v| v.string("number") == id)
        .ok_or("Owned return breakpoint is absent from GDB")?;
    if own.string("type") != "hw breakpoint"
        || own.string("disp") != "del"
        || own.string("enabled") != if disabled { "n" } else { "y" }
        || literal_address(&own.string("addr"))? & !1 != pc & !1
    {
        return Err("Owned return breakpoint identity/state changed".into());
    }
    let mut shared = false;
    for point in points {
        if point.string("number") != id && point.string("type").contains("breakpoint") {
            shared |= site(point, pc)?;
        }
    }
    Ok(shared)
}

impl Engine {
    pub(super) fn prepare_running_return_cancel(&mut self) -> Result<Option<Route>, String> {
        if !matches!(
            self.project.target.mode.as_str(),
            "remote" | "extended-remote"
        ) {
            return Ok(None);
        }
        let core = self.register_context().core;
        let target = self
            .project
            .registers
            .targets
            .get(&core)
            .cloned()
            .ok_or("Running HVC cancellation needs an explicit OpenOCD core target mapping")?;
        let endpoint = self.project.registers.tcl_endpoint.clone();
        if endpoint.is_empty() {
            return Err("Running HVC cancellation needs the OpenOCD TCL endpoint".into());
        }
        if self.console_capture.is_some() {
            return Err("Another GDB console capture is active".into());
        }
        self.console_capture = Some(String::new());
        let result = self.console("info connections");
        let text = self.console_capture.take().unwrap_or_default();
        result?;
        let expected = &self.project.target.endpoint;
        let port = connection_port(&text, expected)?;
        let mut stream = crate::live_watch::connect(&endpoint)?;
        let peer = stream.peer_addr().map_err(|e| e.to_string())?;
        let hosts = expected.to_socket_addrs().map_err(|e| e.to_string())?;
        if !hosts
            .into_iter()
            .any(|a| a.ip() == peer.ip() || a.ip().is_loopback() && peer.ip().is_loopback())
        {
            return Err("OpenOCD TCL peer does not match the current GDB server host".into());
        }
        let target_word = crate::live_watch::word(&target);
        let verified = crate::live_watch::read_only_transaction(
            &mut stream,
            &format!(
                "if {{[{target_word} cget -gdb-port] ne \"{port}\"}} {{error \"GDB/core target port mismatch\"}}; if {{[{target_word} curstate] ne \"halted\"}} {{error \"Return preparation core is not halted\"}}; set __dt_return_verified verified"
            ),
        )?;
        if verified != "verified" {
            return Err("OpenOCD did not verify the return cancellation route".into());
        }
        Ok(Some(Route {
            core,
            endpoint,
            target,
            port,
            peer,
        }))
    }

    pub(super) fn running_return_backend(
        &mut self,
        route: &Route,
        pc: u64,
        shared: bool,
        remove: bool,
    ) -> Result<String, String> {
        if route.core != self.register_context().core {
            return Err("Return cancellation core changed".into());
        }
        let services = crate::debug_access::for_project(&self.project)?;
        let mut leases = services
            .iter()
            .map(|s| s.acquire(false))
            .collect::<Result<Vec<_>, _>>()?;
        let mut stream = crate::live_watch::connect(&route.endpoint)?;
        if stream.peer_addr().map_err(|e| e.to_string())? != route.peer {
            return Err("Return cancellation TCL peer changed".into());
        }
        let target = crate::live_watch::word(&route.target);
        let body = if remove {
            removal(pc & !1, shared)
        } else {
            "set __dt_rc_result running".into()
        };
        let script = format!(
            "set __dt_rc_old [target current]; set __dt_rc_code [catch {{if {{[{target} cget -gdb-port] ne \"{}\"}} {{error \"GDB/core target port mismatch\"}}; set __dt_rc_state [{target} curstate]; if {{$__dt_rc_state eq \"halted\"}} {{set __dt_rc_result halted}} elseif {{$__dt_rc_state eq \"running\"}} {{targets {target}; {body}}} else {{error \"Unknown physical core state\"}}; set __dt_rc_result}} __dt_rc_value]; set __dt_rc_restore [catch {{targets $__dt_rc_old; if {{[target current] ne $__dt_rc_old}} {{error \"target readback mismatch\"}}}} __dt_rc_error]; if {{$__dt_rc_restore}} {{error \"Return cancellation target restoration failed: $__dt_rc_error\"}}; if {{$__dt_rc_code}} {{error $__dt_rc_value}}; set __dt_rc_value",
            route.port
        );
        self.log("guest-return-cancel-tcl>", &script);
        let mut progress = crate::live_watch::TransactionProgress::default();
        let result = if remove {
            crate::live_watch::transact_tracked(&mut stream, &script, &mut progress)
        } else {
            crate::live_watch::read_only_transaction_tracked(&mut stream, &script, &mut progress)
        };
        if let Err(error) = &result
            && progress.started.is_some()
            && (!progress.responded || error.contains("restoration failed"))
        {
            for lease in &mut leases {
                lease.quarantine(error);
            }
            self.register_access_fault = Some(error.clone());
            self.state(state::FAULT);
        }
        if let Ok(value) = &result {
            self.log("guest-return-cancel-tcl<", value);
        }
        result
    }
}

fn removal(pc: u64, shared: bool) -> String {
    let selection = if shared {
        "if {$__dt_rc_count != 1} {error \"Shared physical return site is absent or ambiguous\"}; set __dt_rc_expected $__dt_rc_lines; set __dt_rc_result shared".into()
    } else {
        format!(
            "if {{$__dt_rc_count > 1}} {{error \"Ambiguous physical return breakpoint\"}}; if {{$__dt_rc_count == 1}} {{rbp 0x{pc:x}}}; set __dt_rc_expected $__dt_rc_keep; set __dt_rc_result removed"
        )
    };
    r#"set __dt_rc_before [bp]; set __dt_rc_lines {}; set __dt_rc_keep {}; set __dt_rc_count 0; foreach __dt_rc_line [split $__dt_rc_before "\n"] {if {$__dt_rc_line eq ""} {continue}; lappend __dt_rc_lines $__dt_rc_line; if {[regexp {addr=(0x[0-9a-fA-F]+)} $__dt_rc_line __dt_rc_match __dt_rc_addr] && $__dt_rc_addr == __RETURN_PC__} {if {!__SHARED__ && ![string match "Hardware breakpoint*" $__dt_rc_line]} {error "Return site is not a hardware breakpoint"}; incr __dt_rc_count} else {lappend __dt_rc_keep $__dt_rc_line}}; __SELECTION__; set __dt_rc_after {}; foreach __dt_rc_line [split [bp] "\n"] {if {$__dt_rc_line ne ""} {lappend __dt_rc_after $__dt_rc_line}}; if {$__dt_rc_after ne $__dt_rc_expected} {error "Return breakpoint removal readback changed another site"}"#
        .replace("__RETURN_PC__", &format!("0x{pc:x}"))
        .replace("__SHARED__", if shared { "1" } else { "0" })
        .replace("__SELECTION__", &selection)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_route_requires_one_matching_current_connection() {
        assert_eq!(
            connection_port(
                "Num What Description\n* 1 extended-remote 127.0.0.1:3334 Extended remote target using gdb-specific protocol\n  2 remote other:3333",
                "127.0.0.1:3334"
            )
            .unwrap(),
            3334
        );
        for value in [
            "1 remote 127.0.0.1:3334",
            "* 1 remote other:3334",
            "* 1 native 127.0.0.1:3334",
            "* 1 remote 127.0.0.1:3334\n* 2 remote 127.0.0.1:3334",
        ] {
            assert!(connection_port(value, "127.0.0.1:3334").is_err());
        }
    }
    #[test]
    fn cancellation_does_not_remove_a_shared_conditional_user_site() {
        let record=mi::parse("1^done,BreakpointTable={body=[bkpt={number=\"3\",type=\"hw breakpoint\",enabled=\"y\",addr=\"0x100204ac\",cond=\"0\"},bkpt={number=\"7\",type=\"hw breakpoint\",enabled=\"y\",disp=\"del\",addr=\"0x100204ac\"}]}").unwrap().unwrap();
        assert!(ownership(&record, "7", 0x100204ac, false).unwrap());
        assert!(ownership(&record, "7", 0x100204ac, true).is_err());
        assert!(ownership(&record, "3", 0x100204ac, false).is_err());
        assert!(!removal(0x100204ac, true).contains("rbp "));
        assert!(removal(0x100204ac, false).contains("rbp 0x100204ac"));
        assert!(!removal(0x100204ac, false).contains("halt"));
    }
    #[test]
    fn cancellation_rejects_unknown_user_sites_and_changed_owned_identity() {
        let own =
            r#"bkpt={number="7",type="hw breakpoint",enabled="y",disp="del",addr="0x100204ac"}"#;
        for user in [
            r#"bkpt={number="3",type="breakpoint",enabled="y",addr="<MULTIPLE>",locations=[]}"#,
            r#"bkpt={number="3",type="breakpoint",enabled="y"}"#,
        ] {
            let record = mi::parse(&format!("1^done,BreakpointTable={{body=[{own},{user}]}}"))
                .unwrap()
                .unwrap();
            assert!(ownership(&record, "7", 0x100204ac, false).is_err());
        }
        let record = mi::parse(&format!("1^done,BreakpointTable={{body=[{own}]}}"))
            .unwrap()
            .unwrap();
        assert!(!ownership(&record, "7", 0x100204ad, false).unwrap());
        assert!(ownership(&record, "7", 0x100204b0, false).is_err());
        assert!(ownership(&record, "8", 0x100204ac, false).is_err());
    }
    #[test]
    fn cancellation_checks_each_enabled_multi_location_and_ignores_pending_or_watchpoints() {
        let record = mi::parse(r#"1^done,BreakpointTable={body=[bkpt={number="7",type="hw breakpoint",enabled="y",disp="del",addr="0x100204ac"},bkpt={number="3",type="breakpoint",enabled="y",addr="<MULTIPLE>",locations=[{number="3.1",enabled="n",addr="0x100204ac"},{number="3.2",enabled="y",addr="0x100204ad"}]},bkpt={number="4",type="breakpoint",enabled="y",addr="<PENDING>",pending="missing"},bkpt={number="5",type="hw watchpoint",enabled="y"}]}"#).unwrap().unwrap();
        assert!(ownership(&record, "7", 0x100204ac, false).unwrap());
    }
}
