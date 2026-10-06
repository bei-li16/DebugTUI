//! Per-core overrides preserve omitted defaults and explicit empty selections.
use super::*;

macro_rules! core_config {
    ($($field:ident: $kind:ty),* $(,)?) => {
        #[derive(Clone, Debug, Default, Serialize, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct CoreConfig {
            $(#[serde(skip_serializing_if = "Option::is_none")]
            pub $field: Option<$kind>,)*
        }
        impl CoreConfig {
            pub fn apply(&self, defaults: &Config) -> Config {
                let mut effective = defaults.clone();
                $(if let Some(value) = &self.$field {
                    effective.$field = value.clone();
                })*
                effective
            }
        }
    }
}

// A supplied map replaces that map for this worker. Omitted maps inherit intact.
// Component bindings remain complete typed routes, never partially guessed routes.
core_config! {
    cpu: String,
    catalogue: PathBuf,
    facts: BTreeMap<String, u64>,
    components: BTreeMap<String, Component>,
    component_owners: BTreeMap<String, BTreeMap<String, Component>>,
    mmio_probe: bool,
    tcl_endpoint: String,
    targets: BTreeMap<String, String>,
    cp15_command: String,
    cp15_64_command: String,
    pmu_command: String,
    gic_command: String,
    timer_command: String,
    banked_command: String,
    vfp_command: String,
    vfp_write_command: String,
    selector_command: String,
    isb_command: String,
}
