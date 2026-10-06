//! A PPB address is meaningful only in a named physical core's access view.
use super::*;
use crate::config::MemoryAccess;
pub const START: u64 = 0xe000_0000;
pub const END: u64 = 0xe010_0000;

pub fn gdb_endpoint(
    endpoint: &str,
    observed: Option<&str>,
    cores: &BTreeMap<String, String>,
    core: &str,
) -> Result<(), String> {
    if endpoint.is_empty() || observed != Some(endpoint) {
        return Err("CorePrivate requires this worker's known GDB endpoint; opaque connection changes invalidate it".into());
    }
    if !cores.is_empty()
        && cores
            .get(core)
            .is_none_or(|configured| configured != endpoint)
    {
        return Err("CorePrivate GDB endpoint does not match the selected core".into());
    }
    if cores
        .iter()
        .any(|(name, configured)| name != core && configured == endpoint)
    {
        return Err("CorePrivate cannot use a GDB endpoint shared by multiple configured cores; use an explicit per-core AP channel".into());
    }
    Ok(())
}

pub fn binding<'a>(
    config: &'a Config,
    channels: &[MemoryAccess],
    core: &str,
) -> Result<&'a Component, String> {
    let owner = format!("core:{core}");
    let binding = config.component("ppb", Some(&owner), true)?;
    if binding.base != 0 {
        return Err("CorePrivate ppb uses absolute addresses; binding base must be zero".into());
    }
    if !binding.channel.is_empty() {
        let channel = channels
            .iter()
            .find(|c| c.id == binding.channel)
            .ok_or("CorePrivate memory channel is missing")?;
        if channel.cores != [core] {
            return Err(
                "CorePrivate channel must belong exclusively to the selected physical core".into(),
            );
        }
        if config
            .targets
            .get(core)
            .is_some_and(|target| target != &channel.target)
        {
            return Err(
                "CorePrivate channel target differs from this core's register target".into(),
            );
        }
        if channels.iter().any(|peer| {
            peer.id != channel.id
                && peer.tcl_endpoint == channel.tcl_endpoint
                && peer.target == channel.target
                && peer.cores.iter().any(|name| name != core)
        }) {
            return Err(
                "CorePrivate endpoint/target is also assigned to another physical core".into(),
            );
        }
    }
    Ok(binding)
}
