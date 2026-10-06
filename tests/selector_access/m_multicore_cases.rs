//! Built-in M7/M4 catalogues through the coordinator, MI and actual Tcl flow.
//! Only the local target model supplies bytes; no ARM instruction or board claim.
use super::*;
use debugtui::{
    config::MemoryAccess,
    coordinator,
    registers::{Component, CoreConfig},
};
use std::collections::BTreeMap;

fn pair(shared_service: bool) -> (Fixture, Option<Fixture>, Project) {
    let m7 = m_profile_mpu_cases::m_fixture("cortex-m7", 16);
    let m4 = (!shared_service).then(|| m_profile_mpu_cases::m_fixture("cortex-m4", 8));
    let peer = m4.as_ref().unwrap_or(&m7);
    peer.state.lock().unwrap()["targets"]["cpu1"] =
        json!({"m_cpuid":0x411fc241u32,"m_count":8,"m_rnr":5,"c_sel":0});
    let mut project = m7.project.clone();
    // Independent mock MI processes maintain their own stack frame. The
    // original single-core fault file is deliberately not shared between them.
    project.gdb.env.remove("DEBUGTUI_TEST_CONTEXT_FILE");
    project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_PAUSE".into(), "query-rejected".into());
    project.memory_access.clear();
    project.registers.component_owners.clear();
    project.registers.targets = BTreeMap::from([
        ("core0".into(), "cpu0".into()),
        ("core2".into(), "cpu1".into()),
    ]);
    let first_endpoint = project.target.endpoint.clone();
    let second_endpoint = m4
        .as_ref()
        .map(|f| f.project.target.endpoint.clone())
        .unwrap_or_else(|| {
            let port = first_endpoint
                .rsplit(':')
                .next()
                .unwrap()
                .parse::<u16>()
                .unwrap();
            format!("localhost:{}", port + 10000)
        });
    project.cores = [
        ("core0", "cortex-m7", "cpu0", "ppb7", &m7, first_endpoint),
        ("core2", "cortex-m4", "cpu1", "ppb4", peer, second_endpoint),
    ]
    .into_iter()
    .map(|(name, cpu, target, channel, fixture, endpoint)| {
        let tcl_endpoint = fixture.project.registers.tcl_endpoint.clone();
        project.memory_access.push(MemoryAccess {
            id: channel.into(),
            label: channel.into(),
            tcl_endpoint: tcl_endpoint.clone(),
            target: target.into(),
            cores: vec![name.into()],
            while_running: false,
        });
        Core {
            name: name.into(),
            endpoint,
            registers: Some(CoreConfig {
                cpu: Some(cpu.into()),
                tcl_endpoint: Some(tcl_endpoint),
                component_owners: Some(BTreeMap::from([(
                    "ppb".into(),
                    BTreeMap::from([(
                        format!("core:{name}"),
                        Component {
                            base: 0,
                            channel: channel.into(),
                            little_endian: true,
                        },
                    )]),
                )])),
                ..Default::default()
            }),
            ..Default::default()
        }
    })
    .collect();
    project.validate().unwrap();
    (m7, m4, project)
}

struct Client {
    engine: session::EngineHandle,
    id: u64,
}
impl Client {
    fn new(project: Project) -> Self {
        let mut client = Self {
            engine: coordinator::spawn(project),
            id: 0,
        };
        client.ok("connect", json!({}));
        client.ok("control_scope", json!({"scope":"all"}));
        client
    }
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.id += 1;
        request(&self.engine, self.id, method, params)
    }
    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params)
            .unwrap_or_else(|error| panic!("{method}: {error}"))
    }
    fn select(&mut self, name: &str) -> Value {
        self.ok("select_core", json!({"name":name}));
        self.ok("registers_list", json!({}))
    }
    fn probe(&mut self) -> Value {
        let list = self.ok("registers_list", json!({}));
        self.ok("registers_probe", json!({"context":list["context"]}))
    }
    fn mpu(&mut self, context: &Value, read: bool) -> Value {
        self.ok(
            "registers_mpu",
            json!({"context":context,"bank":"m","read":read}),
        )["view"]
            .clone()
    }
    fn cache(&mut self, context: &Value, read: bool) -> Value {
        self.ok("registers_cache", json!({"context":context,"read":read}))["view"].clone()
    }
    fn cpuid(&mut self, context: &Value) -> Value {
        self.ok(
            "registers_read",
            json!({"context":context,"ids":["scb.cpuid"],"manual":true}),
        )["samples"][0]
            .clone()
    }
}
fn trace(f: &Fixture) -> Vec<Value> {
    f.state.lock().unwrap()["trace"].as_array().unwrap().clone()
}
fn assert_target_since(f: &Fixture, before: usize, target: &str) {
    let trace = trace(f);
    assert!(trace.len() > before, "No physical target reads");
    assert!(
        trace[before..].iter().all(|command| command[0] == target),
        "{trace:?}"
    );
}
fn assert_origin(sample: &Value, core: &str, target: &str, endpoint: &str, hex: &str) {
    assert_eq!(sample["state"], "valid");
    assert_eq!(sample["owner"], format!("core:{core}"));
    assert_eq!(sample["context"]["core"], core);
    assert_eq!(sample["value"]["hex"], hex);
    assert_eq!(sample["provenance"]["access"]["route"]["target"], target);
    assert_eq!(
        sample["provenance"]["access"]["route"]["endpoint"],
        endpoint
    );
}

#[test]
fn m_multicore_builtin_catalogues_ppb_identity_values_and_cache_are_isolated_on_shared_or_distinct_services()
 {
    for shared in [true, false] {
        let (m7, m4, project) = pair(shared);
        let peer = m4.as_ref().unwrap_or(&m7);
        let mut client = Client::new(project);
        let mut views = vec![];
        let mut contexts = vec![];
        for (core, cpu, target, fixture, count, cpuid) in [
            ("core0", "cortex-m7", "cpu0", &m7, 16, "0x410fc271"),
            ("core2", "cortex-m4", "cpu1", peer, 8, "0x411fc241"),
        ] {
            let listed = client.select(core);
            assert_eq!(listed["catalogue"]["cpu"], cpu);
            assert!(listed["probe"].is_null());
            assert_eq!(
                listed["catalogue"]["registers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["id"] == "scb.ccsidr"),
                cpu == "cortex-m7"
            );
            let before = trace(fixture).len();
            let proof = client.probe();
            assert_eq!(proof["facts"]["mpu.regions"], count);
            assert_target_since(fixture, before, target);
            let context = proof["context"].clone();
            let sample = client.cpuid(&context);
            assert_origin(
                &sample,
                core,
                target,
                &fixture.project.registers.tcl_endpoint,
                cpuid,
            );
            assert_eq!(
                sample["provenance"]["access"]["route"]["address"],
                "0xe000ed00"
            );
            let view = client.mpu(&context, true);
            assert_eq!(view["count"], count);
            assert_eq!(view["owner"], format!("core:{core}"));
            assert_eq!(
                view["original_selector"]["hex"],
                if count == 16 {
                    "0x00000003"
                } else {
                    "0x00000005"
                }
            );
            assert_origin(
                &view["regions"][0]["base"],
                core,
                target,
                &fixture.project.registers.tcl_endpoint,
                if target == "cpu0" {
                    "0x20000000"
                } else {
                    "0x30000000"
                },
            );
            views.push(view);
            contexts.push(context);
        }
        client.select("core0");
        let before = trace(&m7);
        assert_eq!(client.mpu(&contexts[0], false), views[0]);
        assert_eq!(trace(&m7), before, "Cached query must not touch PPB");
        let cache = client.cache(&contexts[0], true);
        assert_eq!(cache["owner"], "core:core0");
        client.select("core2");
        let before = trace(peer);
        assert!(
            client
                .call(
                    "registers_cache",
                    json!({"context":contexts[1],"read":true})
                )
                .is_err()
        );
        assert_eq!(
            trace(peer),
            before,
            "M4 must not borrow M7 cache capability"
        );
        assert!(
            client
                .call(
                    "registers_mpu",
                    json!({"context":contexts[0],"bank":"m","read":true})
                )
                .is_err()
        );
        assert_eq!(
            trace(peer),
            before,
            "Foreign core context must be refused before I/O"
        );
        assert_eq!(client.mpu(&contexts[1], false), views[1]);
        client.select("core0");
        assert_eq!(client.cache(&contexts[0], false), cache);
        assert_eq!(m7.state.lock().unwrap()["targets"]["cpu0"]["m_rnr"], 3);
        assert_eq!(peer.state.lock().unwrap()["targets"]["cpu1"]["m_rnr"], 5);
        fs::write(m7.transcript.with_extension("multicore.json"), serde_json::to_vec_pretty(&json!({
            "board_tests_executed":false,"shared_service":shared,"contexts":contexts,"mpu":views,"cache":cache,
            "m7_trace":trace(&m7),"m4_trace":trace(peer)
        })).unwrap()).unwrap();
        client.ok("quit", json!({}));
    }
}

#[test]
fn m_multicore_failure_and_cancel_retain_only_original_owner_and_leave_peer_cache_valid() {
    let (m7, m4, project) = pair(false);
    let m4 = m4.unwrap();
    let mut client = Client::new(project);
    client.select("core0");
    let c7 = client.probe()["context"].clone();
    let original7 = client.mpu(&c7, true);
    client.select("core2");
    let c4 = client.probe()["context"].clone();
    let original4 = client.mpu(&c4, true);
    let peer_trace = trace(&m4);
    client.select("core0");
    m7.state.lock().unwrap()["fault"] = json!("m_base_read");
    assert!(
        client
            .call(
                "registers_mpu",
                json!({"context":c7,"bank":"m","read":true})
            )
            .is_err()
    );
    let failed = client.ok("status", json!({}))["register_mpu"].clone();
    assert!(client.ok("status", json!({}))["register_probe"].is_null());
    assert_eq!(failed["state"], "stale");
    for key in ["value", "context", "timestamp_ms", "provenance", "owner"] {
        assert_eq!(
            failed["regions"][0]["base"][key],
            original7["regions"][0]["base"][key]
        );
    }
    assert_eq!(trace(&m4), peer_trace);
    client.select("core2");
    assert_eq!(client.mpu(&c4, false), original4);
    assert_eq!(trace(&m4), peer_trace);
    client.select("core0");
    m7.state.lock().unwrap()["fault"] = json!("");
    let before = trace(&m7);
    let denied = client
        .call(
            "registers_mpu",
            json!({"context":c7,"bank":"m","read":true}),
        )
        .unwrap_err();
    assert!(denied.contains("Probe"), "{denied}");
    assert_eq!(
        trace(&m7),
        before,
        "Known failure revoked only the failing core's proof"
    );
    assert_eq!(client.probe()["context"], c7);
    client.id += 1;
    let request = Request::new(
        client.id,
        "registers_mpu",
        json!({"context":c7,"bank":"m","read":true}),
    );
    *m7.cancel_on_transaction.lock().unwrap() = Some(request.clone());
    client.engine.send(request).unwrap();
    loop {
        if let Event::Response { id, ok, error, .. } = client
            .engine
            .events
            .recv_timeout(Duration::from_secs(15))
            .unwrap()
            && id == client.id
        {
            assert!(
                !ok && error
                    .as_deref()
                    .is_some_and(|error| error.contains("cancelled")),
                "ok={ok} error={error:?}"
            );
            break;
        }
    }
    assert_eq!(m7.state.lock().unwrap()["targets"]["cpu0"]["m_rnr"], 3);
    assert_eq!(trace(&m4), peer_trace);
    client.select("core2");
    assert_eq!(client.mpu(&c4, false), original4);
    client.ok("quit", json!({}));
}

#[test]
fn m_multicore_frame_run_boundary_and_reconnect_expire_only_applicable_samples_without_peer_leaks()
{
    let (m7, m4, project) = pair(false);
    let m4 = m4.unwrap();
    let mut client = Client::new(project);
    client.select("core0");
    let c7 = client.probe()["context"].clone();
    let old7 = client.mpu(&c7, true);
    let cache7 = client.cache(&c7, true);
    client.select("core2");
    let c4 = client.probe()["context"].clone();
    let old4 = client.mpu(&c4, true);
    let before4 = trace(&m4);
    client.select("core0");
    client.ok("control_scope", json!({"scope":"core"}));
    client.ok("frame", json!({"level":1}));
    let frame = client.ok("status", json!({}));
    assert_eq!(frame["register_cache"]["state"], "stale");
    assert_eq!(frame["register_mpu"]["state"], "stale");
    assert_eq!(
        frame["register_cache"]["caches"][0]["size_id"]["provenance"],
        cache7["caches"][0]["size_id"]["provenance"]
    );
    client.select("core2");
    assert_eq!(client.mpu(&c4, false), old4);
    assert_eq!(trace(&m4), before4);
    client.select("core0");
    client.ok("frame", json!({"level":0}));
    client.ok("continue", json!({}));
    let running = client.ok("status", json!({}));
    assert_eq!(running["state"], "RUNNING");
    assert_eq!(running["register_mpu"]["state"], "stale");
    assert_eq!(
        running["register_mpu"]["regions"][0]["base"]["value"],
        old7["regions"][0]["base"]["value"]
    );
    let before7 = trace(&m7);
    assert!(
        client
            .call(
                "registers_mpu",
                json!({"context":c7,"bank":"m","read":true})
            )
            .is_err()
    );
    assert_eq!(trace(&m7), before7);
    client.select("core2");
    assert_eq!(client.mpu(&c4, false), old4);
    assert_eq!(trace(&m4), before4);
    assert_eq!(client.ok("status", json!({}))["state"], "STOPPED");
    client.select("core0");
    client.ok("pause", json!({}));
    let fresh = client.probe()["context"].clone();
    assert_ne!(fresh["generation"], c7["generation"]);
    client.ok("register_boundary", json!({}));
    client.select("core2");
    assert_eq!(client.mpu(&c4, false), old4);
    client.ok("reconnect", json!({}));
    for (core, previous, count) in [("core0", &c7, 16), ("core2", &c4, 8)] {
        let listed = client.select(core);
        assert_ne!(listed["context"]["session"], previous["session"]);
        assert!(listed["probe"].is_null());
        assert!(client.ok("status", json!({}))["register_mpu"].is_null());
        let proof = client.probe();
        assert_eq!(proof["facts"]["mpu.regions"], count);
        assert!(
            client
                .call(
                    "registers_mpu",
                    json!({"context":previous,"bank":"m","read":true})
                )
                .is_err()
        );
    }
    client.ok("quit", json!({}));
}

#[test]
fn m_multicore_new_identity_revokes_only_this_cores_capacity_and_indexed_values() {
    let (m7, m4, project) = pair(false);
    let m4 = m4.unwrap();
    let mut client = Client::new(project);
    client.select("core0");
    let c7 = client.probe()["context"].clone();
    let old7 = client.mpu(&c7, true);
    let old_cache = client.cache(&c7, true);
    client.select("core2");
    let c4 = client.probe()["context"].clone();
    let old4 = client.mpu(&c4, true);
    let peer_trace = trace(&m4);
    client.select("core0");
    m7.state.lock().unwrap()["targets"]["cpu0"]["m_cpuid"] = json!(0x411fc241u32);
    let changed = client.cpuid(&c7);
    assert_origin(
        &changed,
        "core0",
        "cpu0",
        &m7.project.registers.tcl_endpoint,
        "0x411fc241",
    );
    let listed = client.ok("registers_list", json!({}));
    assert!(listed["facts"]["mpu.regions"].is_null());
    let stale = client.ok("status", json!({}));
    assert_eq!(stale["register_mpu"]["state"], "stale");
    assert_eq!(stale["register_cache"]["state"], "stale");
    assert_eq!(
        stale["register_mpu"]["regions"][0]["base"]["provenance"],
        old7["regions"][0]["base"]["provenance"]
    );
    assert_eq!(
        stale["register_cache"]["caches"][0]["size_id"]["value"],
        old_cache["caches"][0]["size_id"]["value"]
    );
    let before = trace(&m7);
    assert!(
        client
            .call(
                "registers_mpu",
                json!({"context":c7,"bank":"m","read":true})
            )
            .is_err()
    );
    assert!(
        client
            .call("registers_cache", json!({"context":c7,"read":true}))
            .is_err()
    );
    assert_eq!(
        trace(&m7),
        before,
        "Changed identity must not lend old indexed capability"
    );
    client.select("core2");
    assert_eq!(client.mpu(&c4, false), old4);
    assert_eq!(
        client.ok("registers_list", json!({}))["facts"]["mpu.regions"],
        8
    );
    assert_eq!(trace(&m4), peer_trace);
    client.ok("quit", json!({}));
}

#[test]
fn m_multicore_deferred_driver_runs_actual_exe_and_wrong_baseline_stops_before_region_writes() {
    let (m7, m4, project) = pair(false);
    let m4 = m4.unwrap();
    let out = m7.transcript.parent().unwrap();
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts/test-m-profile-multicore-hardware.cjs");
    let default = Command::new("node").arg(&script).output().unwrap();
    assert!(default.status.success());
    assert!(
        String::from_utf8_lossy(&default.stdout)
            .contains("\"passed\":0,\"failed\":0,\"skipped\":4")
    );
    assert!(trace(&m7).is_empty() && trace(&m4).is_empty());
    let project_path = out.join("multicore-driver.toml");
    fs::write(&project_path, toml::to_string(&project).unwrap()).unwrap();
    let original = fs::read(&project_path).unwrap();
    let mut spec: Value = serde_json::from_str(include_str!(
        "../fixtures/m-profile-multicore-board.example.json"
    ))
    .unwrap();
    spec["cores"][0]["endpoint"] = json!(m7.project.registers.tcl_endpoint);
    spec["cores"][1]["endpoint"] = json!(m4.project.registers.tcl_endpoint);
    let case_path = out.join("multicore-driver.json");
    fs::write(&case_path, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
    let run = || {
        Command::new("node")
            .arg(&script)
            .args([
                "--run",
                "--software-fixture",
                "--binary",
                env!("CARGO_BIN_EXE_debugtui"),
                "--project",
            ])
            .arg(&project_path)
            .arg("--case")
            .arg(&case_path)
            .env("DEBUGTUI_CONFIG_DIR", out.join("private-config"))
            .output()
            .unwrap()
    };
    let result = run();
    fs::write(
        out.join("multicore-driver-positive.log"),
        [result.stdout.clone(), result.stderr.clone()].concat(),
    )
    .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stdout).contains("\"passed\":5,\"failed\":0,\"skipped\":0")
    );
    assert_eq!(fs::read(&project_path).unwrap(), original);
    for (fixture, target) in [(&m7, "cpu0"), (&m4, "cpu1")] {
        assert!(
            trace(fixture)
                .iter()
                .all(|operation| operation[0] == target)
        );
        assert!(
            trace(fixture)
                .iter()
                .filter(|operation| operation[1] == "write_memory")
                .all(|operation| operation[2] == "0xe000ed98")
        );
        fixture.state.lock().unwrap()["trace"] = json!([]);
    }
    spec["cores"][0]["cpuid"] = json!("0x411fc241");
    fs::write(&case_path, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
    let wrong = run();
    fs::write(
        out.join("multicore-driver-wrong.log"),
        [wrong.stdout.clone(), wrong.stderr.clone()].concat(),
    )
    .unwrap();
    assert!(!wrong.status.success());
    assert!(String::from_utf8_lossy(&wrong.stderr).contains("M-MULTI-H01-IDENTITY"));
    assert!(
        String::from_utf8_lossy(&wrong.stdout).contains("\"passed\":1,\"failed\":1,\"skipped\":3")
    );
    assert!(
        trace(&m7)
            .iter()
            .all(|operation| operation[1] == "read_memory")
    );
    assert!(
        trace(&m4).is_empty(),
        "Wrong first-core baseline must not read peer values"
    );
    assert_eq!(fs::read(&project_path).unwrap(), original);
}
