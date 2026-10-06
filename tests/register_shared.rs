#![cfg(windows)]
//! Actual four-worker MI pipes; register ownership is independently checked against board bytes.
use debugtui::{
    config::{Core, Project},
    coordinator,
    session::{EngineHandle, Event, Request},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn fixture(name: &str, chip: bool) -> (Project, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = root
        .join("artifacts")
        .join(format!("register shared {name} {}", std::process::id()));
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("commands.txt"), "").unwrap();
    fs::write(out.join("memory.json"),json!({
        "localhost:4330":{"0x20000000":"11111111","0x20000004":"aaaabbbb","0x20000008":"ccddee11"},
        "localhost:4331":{"0x20000000":"22222222","0x20000004":"aaaabbbb","0x20000008":"ccddee11"},
        "localhost:4332":{"0x20000000":"33333333","0x20000004":"bbbbcccc","0x20000008":"ccddee11"},
        "localhost:4333":{"0x20000000":"44444444","0x20000004":"deadbeef","0x20000008":"ccddee11"}
    }).to_string()).unwrap();
    let catalogue = out.join("catalogue.toml");
    fs::write(
        &catalogue,
        r#"
version=1
cpu="cortex-r52"
architecture="armv8-r-aarch32"
[[groups]]
id="scope"
name="Board scopes"
[[registers]]
id="private"
name="Private"
group="scope"
bits=32
access="ro"
reader={kind="mmio",component="board",offset=0}
[[registers]]
id="cluster"
name="Cluster"
group="scope"
bits=32
access="ro"
scope="cluster"
reader={kind="mmio",component="board",offset=4}
[[registers]]
id="cluster_alias"
name="Cluster alias"
group="scope"
bits=16
access="ro"
scope="cluster"
reader={kind="alias",source="cluster",offset=8}
[[registers]]
id="chip"
name="Chip"
group="scope"
bits=32
access="ro"
scope="chip"
reader={kind="mmio",component="board",offset=8}
[[registers]]
id="chip_alias"
name="Chip alias"
group="scope"
bits=8
access="ro"
scope="chip"
reader={kind="alias",source="chip",offset=16}
"#,
    )
    .unwrap();
    let mut project = Project::default();
    project.gdb.executable = "node".into();
    project.gdb.args = vec![
        root.join("tests/mock-gdb.cjs")
            .to_string_lossy()
            .into_owned(),
    ];
    project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_REGISTERS".into(), json!(["r0"]).to_string());
    project.gdb.env.insert(
        "DEBUGTUI_TEST_TRANSCRIPT".into(),
        out.join("commands.txt").to_string_lossy().into_owned(),
    );
    project.gdb.env.insert(
        "DEBUGTUI_TEST_OWNER_MEMORY_FILE".into(),
        out.join("memory.json").to_string_lossy().into_owned(),
    );
    project.cores = (0..4)
        .map(|i| Core {
            name: format!("core{i}"),
            endpoint: format!("localhost:{}", 4330 + i),
            ..Default::default()
        })
        .collect();
    project.session.on_exit = "disconnect".into();
    project.debug.chip = String::new();
    project.registers.catalogue = catalogue;
    project.registers.topology.chip = if chip { "board".into() } else { String::new() };
    project.registers.topology.clusters = [
        ("core0".into(), "A".into()),
        ("core1".into(), "A".into()),
        ("core2".into(), "B".into()),
    ]
    .into();
    project.registers.components.insert(
        "board".into(),
        debugtui::registers::Component {
            base: 0x20000000,
            channel: String::new(),
            little_endian: true,
        },
    );
    (project, out)
}
fn response(engine: &EngineHandle, id: u64, expected_ok: bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Event::Response {
            id: found,
            ok,
            result,
            error,
        } = engine
            .events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap()
            && found == id
        {
            assert_eq!(ok, expected_ok, "{id}: {error:?}; {result}");
            return if ok { result } else { json!({"error":error}) };
        }
    }
}
fn call(engine: &EngineHandle, id: u64, method: &str, params: Value) -> Value {
    engine.send(Request::new(id, method, params)).unwrap();
    response(engine, id, true)
}
fn select(engine: &EngineHandle, core: u64) {
    call(engine, 100 + core, "select_core", json!({"index":core}));
}
fn sample<'a>(snapshot: &'a Value, id: &str) -> &'a Value {
    snapshot["register_samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == id)
        .unwrap()
}
fn reads(out: &Path) -> usize {
    fs::read_to_string(out.join("commands.txt"))
        .unwrap()
        .lines()
        .filter(|c| c.starts_with("-data-read-memory-bytes "))
        .count()
}

#[test]
fn register_matrix_scope_all_uses_one_core_and_expires_shared_receipts_after_peer_activity() {
    let (mut project, out) = fixture("matrix", true);
    // Keep the peer running during the export. The fixture's default 10 ms
    // automatic stop queues unrelated stack refreshes after the log checkpoint.
    project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_PAUSE".into(), "event".into());
    let engine = coordinator::spawn(project);
    call(&engine, 1, "connect", json!({}));
    call(&engine, 2, "control_scope", json!({"scope":"all"}));
    select(&engine, 1);
    let baseline = call(
        &engine,
        3,
        "registers_read",
        json!({"ids":["private","cluster_alias","chip_alias"],"scope":"all"}),
    );
    let commands = fs::read_to_string(out.join("commands.txt")).unwrap();
    let matrix = call(&engine, 4, "registers_matrix", json!({"scope":"all"}));
    assert_eq!(matrix["context"]["core"], "core1");
    for id in ["private", "cluster_alias", "chip_alias"] {
        let row = matrix["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .unwrap();
        assert_eq!(row["support"], "observed_value", "{row}");
        let observed = &matrix["observations"][row["observation"].as_u64().unwrap() as usize];
        assert_eq!(
            observed["provenance"]["access"]["route"]["endpoint"],
            "localhost:4331"
        );
        if id != "private" {
            assert_eq!(
                observed["owner_generation"],
                matrix["owner_generations"][observed["owner"].as_str().unwrap()]
            );
        }
    }
    assert_eq!(
        fs::read_to_string(out.join("commands.txt")).unwrap(),
        commands
    );
    select(&engine, 0);
    call(&engine, 5, "continue", json!({"scope":"core"}));
    select(&engine, 1);
    let status = call(&engine, 50, "status", json!({}));
    assert!(
        status["cores"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "core0" && c["state"] == "RUNNING")
    );
    let commands = fs::read_to_string(out.join("commands.txt")).unwrap();
    let expired = call(&engine, 6, "registers_matrix", json!({}));
    for id in ["private", "cluster_alias", "chip_alias"] {
        let row = expired["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .unwrap();
        assert_eq!(
            row["support"],
            if id == "private" {
                "observed_value"
            } else {
                "stale"
            }
        );
        let observed = &expired["observations"][row["observation"].as_u64().unwrap() as usize];
        assert_eq!(
            observed["value"],
            baseline["samples"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["id"] == id)
                .unwrap()["value"]
        );
    }
    assert_eq!(
        fs::read_to_string(out.join("commands.txt")).unwrap(),
        commands
    );
    select(&engine, 3);
    let unknown = call(&engine, 7, "registers_matrix", json!({}));
    assert_eq!(
        unknown["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == "cluster")
            .unwrap()["support"],
        "unknown_owner"
    );
    call(&engine, 8, "quit", json!({}));
}

#[test]
fn four_core_cluster_chip_alias_and_unknown_owners_use_exact_routes_without_cross_cluster_values() {
    for chip in [true, false] {
        let (project, out) = fixture(&format!("owners-{chip}"), chip);
        let engine = coordinator::spawn(project);
        call(&engine, 1, "connect", json!({}));
        let metadata = call(&engine, 20, "registers_list", json!({}));
        assert_eq!(metadata["topology_source"], "configuration");
        assert_eq!(metadata["topology"]["clusters"]["core0"], "A");
        assert_eq!(metadata["topology"]["clusters"]["core2"], "B");
        assert!(metadata["topology"]["clusters"]["core3"].is_null());
        let mut expected_reads = 0;
        let mut contexts = vec![];
        for core in 0..4 {
            select(&engine, core);
            let read = call(
                &engine,
                2,
                "registers_read",
                json!({"ids":["private","cluster_alias","cluster","chip_alias","chip"],"scope":"all"}),
            );
            contexts.push(read["context"].clone());
            let samples = read["samples"].as_array().unwrap();
            for (index, address) in [
                (0, "0x20000000"),
                (1, "0x20000004"),
                (2, "0x20000004"),
                (3, "0x20000008"),
                (4, "0x20000008"),
            ] {
                let access = &samples[index]["provenance"]["access"];
                if (index == 1 || index == 2) && core == 3 || (index == 3 || index == 4) && !chip {
                    assert!(
                        access.is_null(),
                        "unknown owners must not inherit a previous row's route"
                    );
                    continue;
                }
                assert_eq!(access["route"]["kind"], "gdb_memory");
                assert_eq!(
                    access["route"]["endpoint"],
                    format!("localhost:{}", 4330 + core)
                );
                assert_eq!(access["route"]["address"], address);
                assert_eq!(access["route"]["bits"], 32);
                assert_eq!(access["route"]["byte_order"], "little");
                assert_eq!(access["context"], read["context"]);
                assert_eq!(access["phase"], "responded");
                assert_eq!(
                    access["command"],
                    format!("-data-read-memory-bytes {address} 4")
                );
            }
            if core != 3 {
                assert_eq!(
                    samples[1]["provenance"]["access"],
                    samples[2]["provenance"]["access"]
                );
            }
            if chip {
                assert_eq!(
                    samples[3]["provenance"]["access"],
                    samples[4]["provenance"]["access"]
                );
            }
            for value in samples {
                if let Some(owner) = value["owner"].as_str() {
                    if owner.starts_with("core:") {
                        assert!(value["owner_generation"].is_null());
                    } else {
                        assert_eq!(value["owner_generation"], read["owner_generations"][owner]);
                    }
                }
            }
            assert_eq!(samples[0]["owner"], format!("core:core{core}"));
            assert_eq!(
                samples[0]["value"]["hex"],
                format!("0x{:08x}", 0x11111111u32 * (core as u32 + 1))
            );
            expected_reads += 1;
            for index in [1, 2] {
                if core == 3 {
                    assert!(samples[index]["owner"].is_null());
                    assert_eq!(samples[index]["state"], "unavailable");
                    assert!(samples[index]["value"].is_null());
                    assert!(
                        samples[index]["detail"]
                            .as_str()
                            .unwrap()
                            .contains("owner is unknown")
                    );
                } else {
                    assert_eq!(
                        samples[index]["owner"],
                        if core == 2 { "cluster:B" } else { "cluster:A" }
                    );
                    assert_eq!(samples[index]["state"], "valid");
                }
            }
            if core != 3 {
                assert_eq!(
                    samples[2]["value"]["hex"],
                    if core == 2 {
                        "0xccccbbbb"
                    } else {
                        "0xbbbbaaaa"
                    }
                );
                assert_eq!(
                    samples[1]["value"]["hex"],
                    if core == 2 { "0xccbb" } else { "0xbbaa" }
                );
                expected_reads += 1;
            }
            for index in [3, 4] {
                assert_eq!(
                    samples[index]["state"],
                    if chip { "valid" } else { "unavailable" }
                );
                if chip {
                    assert_eq!(samples[index]["owner"], "chip:board");
                } else {
                    assert!(samples[index]["owner"].is_null() && samples[index]["value"].is_null());
                }
            }
            if chip {
                assert_eq!(samples[4]["value"]["hex"], "0x11eeddcc");
                assert_eq!(samples[3]["value"]["hex"], "0xee");
                expected_reads += 1;
            }
            assert_eq!(
                reads(&out),
                expected_reads,
                "aliases must share one physical parent read, and Scope All must not broadcast"
            );
            let snapshot = call(&engine, 3, "status", json!({}));
            assert_eq!(
                snapshot["register_owner_generations"],
                read["owner_generations"]
            );
            assert_eq!(sample(&snapshot, "private")["value"], samples[0]["value"]);
            assert_eq!(sample(&snapshot, "cluster")["state"], samples[2]["state"]);
        }
        assert_ne!(contexts[0]["session"], contexts[1]["session"]);
        for core in [0, 1, 2] {
            select(&engine, core);
            let snapshot = call(&engine, 4, "status", json!({}));
            assert_eq!(sample(&snapshot, "cluster")["state"], "valid");
            assert_eq!(
                sample(&snapshot, "cluster")["context"]["core"],
                format!("core{core}")
            );
        }
        assert_eq!(reads(&out), expected_reads);
        call(&engine, 5, "quit", json!({}));
    }
}

#[test]
fn a_peer_run_expires_its_cluster_and_chip_but_not_another_cluster_or_private_samples() {
    let (project, out) = fixture("boundaries", true);
    let engine = coordinator::spawn(project);
    call(&engine, 1, "connect", json!({}));
    for core in 0..4 {
        select(&engine, core);
        call(
            &engine,
            2,
            "registers_read",
            json!({"ids":["private","cluster","chip"]}),
        );
    }
    let before = reads(&out);
    select(&engine, 1);
    call(&engine, 3, "continue", json!({"scope":"core"}));
    call(&engine, 4, "pause", json!({"scope":"core"}));
    for (core, cluster_state) in [(0, "stale"), (2, "valid")] {
        select(&engine, core);
        let snapshot = call(&engine, 5, "status", json!({}));
        assert_eq!(sample(&snapshot, "private")["state"], "valid");
        assert_eq!(sample(&snapshot, "cluster")["state"], cluster_state);
        assert_eq!(sample(&snapshot, "chip")["state"], "stale");
        assert_eq!(sample(&snapshot, "chip")["value"]["hex"], "0x11eeddcc");
    }
    assert_eq!(
        reads(&out),
        before,
        "lifecycle changes must not implicitly resample peers"
    );
    select(&engine, 0);
    let fresh = call(
        &engine,
        6,
        "registers_read",
        json!({"ids":["cluster","cluster_alias"]}),
    );
    assert_eq!(fresh["samples"][0]["state"], "valid");
    assert_eq!(reads(&out), before + 1);
    select(&engine, 3);
    call(&engine, 7, "continue", json!({"scope":"core"}));
    call(&engine, 8, "pause", json!({"scope":"core"}));
    for core in [0, 2] {
        select(&engine, core);
        let snapshot = call(&engine, 9, "status", json!({}));
        assert_eq!(
            sample(&snapshot, "cluster")["state"],
            "stale",
            "an unmapped core cannot be presumed outside either cluster"
        );
        assert_eq!(sample(&snapshot, "private")["state"], "valid");
    }
    assert_eq!(reads(&out), before + 1);
    select(&engine, 0);
    let old = call(&engine, 10, "registers_list", json!({}))["context"].clone();
    call(&engine, 11, "reconnect", json!({}));
    for core in 0..4 {
        select(&engine, core);
        let snapshot = call(&engine, 12, "status", json!({}));
        assert!(snapshot["register_samples"].as_array().unwrap().is_empty());
        assert_ne!(snapshot["register_session"], old["session"]);
    }
    select(&engine, 0);
    engine
        .send(Request::new(
            13,
            "registers_read",
            json!({"ids":["cluster"],"context":old}),
        ))
        .unwrap();
    let expired = response(&engine, 13, false);
    assert!(expired["error"].as_str().unwrap().contains("expired"));
    assert_eq!(reads(&out), before + 1);
    let fresh = call(&engine, 14, "registers_read", json!({"ids":["cluster"]}));
    assert_eq!(fresh["samples"][0]["state"], "valid");
    assert_eq!(
        fresh["samples"][0]["owner_generation"],
        fresh["owner_generations"]["cluster:A"]
    );
    call(&engine, 15, "quit", json!({}));
}

#[test]
fn peer_activity_during_a_shared_read_discards_new_bytes_and_preserves_last_accepted_sample() {
    let (mut project, out) = fixture("race", true);
    let notifications = out.join("notify.json");
    fs::write(&notifications, "{}").unwrap();
    project.gdb.env.insert(
        "DEBUGTUI_TEST_NOTIFY_FILE".into(),
        notifications.to_string_lossy().into_owned(),
    );
    project
        .gdb
        .env
        .insert("DEBUGTUI_TEST_OWNER_MEMORY_DELAY_MS".into(), "350".into());
    let engine = coordinator::spawn(project);
    call(&engine, 1, "connect", json!({}));
    select(&engine, 0);
    let baseline = call(&engine, 2, "registers_read", json!({"ids":["cluster"]}));
    assert_eq!(baseline["samples"][0]["state"], "valid");
    let mut bytes: Value =
        serde_json::from_slice(&fs::read(out.join("memory.json")).unwrap()).unwrap();
    bytes["localhost:4330"]["0x20000004"] = json!("efbeadde");
    fs::write(out.join("memory.json"), bytes.to_string()).unwrap();
    engine
        .send(Request::new(
            3,
            "registers_read",
            json!({"ids":["private","cluster"]}),
        ))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while reads(&out) != 2 {
        assert!(Instant::now() < deadline, "delayed read did not start");
        std::thread::sleep(Duration::from_millis(5));
    }
    fs::write(
        &notifications,
        json!({"localhost:4331":"running"}).to_string(),
    )
    .unwrap();
    let mut peer_ran = false;
    let result = loop {
        match engine
            .events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap()
        {
            Event::Snapshot { snapshot } => {
                peer_ran |= snapshot
                    .cores
                    .iter()
                    .any(|c| c.name == "core1" && c.state == "RUNNING")
            }
            Event::Response {
                id: 3,
                ok,
                result,
                error,
            } => {
                assert!(ok, "{error:?}");
                break result;
            }
            _ => {}
        }
    };
    assert!(
        peer_ran,
        "peer notification must be observed before accepting the delayed batch"
    );
    assert_eq!(result["samples"][0]["state"], "valid");
    let expired = &result["samples"][1];
    assert_eq!(expired["state"], "stale");
    assert_eq!(
        expired["last_value_provenance"],
        json!({"status":"known","provenance":baseline["samples"][0]["provenance"]})
    );
    assert_eq!(expired["provenance"]["access"]["phase"], "responded");
    assert_ne!(
        expired["provenance"]["access"],
        baseline["samples"][0]["provenance"]["access"]
    );
    assert_eq!(expired["value"], baseline["samples"][0]["value"]);
    assert_eq!(
        expired["last_value_eligibility"],
        json!({"kind":"known","evidence":baseline["samples"][0]["eligibility"]})
    );
    assert_eq!(
        expired["timestamp_ms"],
        baseline["samples"][0]["timestamp_ms"]
    );
    assert!(
        expired["detail"]
            .as_str()
            .unwrap()
            .contains("new value discarded")
    );
    assert_ne!(
        expired["owner_generation"],
        result["owner_generations"]["cluster:A"]
    );
    let snapshot = call(&engine, 4, "status", json!({}));
    assert_eq!(
        sample(&snapshot, "cluster")["value"],
        baseline["samples"][0]["value"]
    );
    assert_eq!(sample(&snapshot, "cluster")["state"], "stale");
    assert_eq!(sample(&snapshot, "private")["state"], "valid");
    assert_eq!(reads(&out), 3);
    // The worker still holds rejected bytes; a subsequent failed refresh must
    // never publish those bytes or their origin as the last accepted sample.
    bytes["localhost:4330"]
        .as_object_mut()
        .unwrap()
        .remove("0x20000004");
    fs::write(out.join("memory.json"), bytes.to_string()).unwrap();
    engine
        .send(Request::new(
            6,
            "registers_read",
            json!({"ids":["cluster"]}),
        ))
        .unwrap();
    loop {
        match engine.events.recv_timeout(Duration::from_secs(15)).unwrap() {
            Event::Snapshot { snapshot }
                if snapshot.core.as_ref().is_some_and(|c| c.name == "core0") =>
            {
                for value in &snapshot.register_samples {
                    if value.id == "cluster" && value.state == debugtui::registers::State::Error {
                        let value = serde_json::to_value(value).unwrap();
                        assert_eq!(
                            value["value"], baseline["samples"][0]["value"],
                            "even an intermediate failure snapshot must reject the discarded bytes"
                        );
                        assert_eq!(
                            value["last_value_eligibility"],
                            json!({"kind":"known","evidence":baseline["samples"][0]["eligibility"]})
                        );
                        assert_eq!(
                            value["last_value_provenance"],
                            json!({"status":"known","provenance":baseline["samples"][0]["provenance"]})
                        );
                    }
                }
            }
            Event::Response {
                id: 6,
                ok,
                result,
                error,
            } => {
                assert!(ok, "{error:?}");
                assert_eq!(result["samples"][0]["state"], "error");
                assert_eq!(
                    result["samples"][0]["value"],
                    baseline["samples"][0]["value"]
                );
                break;
            }
            _ => {}
        }
    }
    call(&engine, 7, "frame", json!({"level":0}));
    let failure_snapshot = call(&engine, 8, "status", json!({}));
    assert_eq!(
        sample(&failure_snapshot, "cluster")["value"],
        baseline["samples"][0]["value"]
    );
    assert_eq!(
        sample(&failure_snapshot, "cluster")["last_value_provenance"],
        json!({"status":"known","provenance":baseline["samples"][0]["provenance"]})
    );
    assert_eq!(
        sample(&failure_snapshot, "cluster")["last_value_eligibility"],
        json!({"kind":"known","evidence":baseline["samples"][0]["eligibility"]})
    );
    assert_eq!(reads(&out), 4);
    fs::write(
        &notifications,
        json!({"localhost:4331":"stopped"}).to_string(),
    )
    .unwrap();
    call(&engine, 5, "quit", json!({}));
}

#[test]
fn shared_failures_keep_last_values_and_current_failure_lifetime_through_later_snapshots() {
    let (project, out) = fixture("errors", true);
    let engine = coordinator::spawn(project);
    call(&engine, 1, "connect", json!({}));
    select(&engine, 0);
    let baseline = call(&engine, 2, "registers_read", json!({"ids":["cluster"]}));
    let mut bytes: Value =
        serde_json::from_slice(&fs::read(out.join("memory.json")).unwrap()).unwrap();
    bytes["localhost:4330"]
        .as_object_mut()
        .unwrap()
        .remove("0x20000004");
    fs::write(out.join("memory.json"), bytes.to_string()).unwrap();
    let failure = call(&engine, 3, "registers_read", json!({"ids":["cluster"]}));
    assert_eq!(failure["samples"][0]["state"], "error");
    assert_eq!(
        failure["samples"][0]["value"],
        baseline["samples"][0]["value"]
    );
    assert_eq!(
        failure["samples"][0]["owner_generation"],
        failure["owner_generations"]["cluster:A"]
    );
    call(&engine, 4, "frame", json!({"level":0}));
    let snapshot = call(&engine, 5, "status", json!({}));
    let cached = sample(&snapshot, "cluster");
    assert_eq!(cached["state"], "error");
    assert_eq!(cached["value"], baseline["samples"][0]["value"]);
    assert_eq!(
        cached["owner_generation"],
        snapshot["register_owner_generations"]["cluster:A"]
    );
    assert_eq!(reads(&out), 2);
    call(&engine, 6, "reconnect", json!({}));
    select(&engine, 0);
    let empty = call(&engine, 7, "status", json!({}));
    assert!(empty["register_samples"].as_array().unwrap().is_empty());
    let unproven = call(&engine, 8, "registers_read", json!({"ids":["cluster"]}));
    assert_eq!(unproven["samples"][0]["state"], "error");
    assert!(
        unproven["samples"][0]["value"].is_null(),
        "a new session must never recover old raw bytes from the coordinator cache"
    );
    assert_eq!(reads(&out), 3);
    call(&engine, 9, "quit", json!({}));
}

#[test]
fn a_peer_stop_with_unchanged_state_publishes_shared_invalidation_without_a_new_read() {
    let (mut project, out) = fixture("stop-again", true);
    let catalogue = fs::read_to_string(&project.registers.catalogue).unwrap();
    fs::write(
        &project.registers.catalogue,
        format!("{catalogue}\n[[registers]]\nid=\"cluster_gdb\"\nname=\"Shared GDB\"\ngroup=\"scope\"\nbits=32\naccess=\"ro\"\nscope=\"cluster\"\nreader={{kind=\"gdb\",name=\"r0\"}}\n"),
    )
    .unwrap();
    project.gdb.env.insert(
        "DEBUGTUI_TEST_REGISTER_VALUES".into(),
        json!({"r0":"0x01020304"}).to_string(),
    );
    let notifications = out.join("notify.json");
    fs::write(&notifications, "{}").unwrap();
    project.gdb.env.insert(
        "DEBUGTUI_TEST_NOTIFY_FILE".into(),
        notifications.to_string_lossy().into_owned(),
    );
    let engine = coordinator::spawn(project);
    call(&engine, 1, "connect", json!({}));
    select(&engine, 0);
    let baseline = call(
        &engine,
        2,
        "registers_read",
        json!({"ids":["private","cluster","cluster_gdb"]}),
    );
    assert_eq!(baseline["samples"][2]["state"], "valid");
    assert_eq!(baseline["samples"][2]["value"]["hex"], "0x01020304");
    fs::write(
        &notifications,
        json!({"localhost:4331":"stop-again"}).to_string(),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let changed = loop {
        if let Event::Snapshot { snapshot } = engine
            .events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap()
            && snapshot.register_owner_generations["cluster:A"]
                > baseline["samples"][1]["owner_generation"].as_u64().unwrap()
        {
            break snapshot;
        }
    };
    assert_eq!(changed.cores[1].state, "STOPPED");
    assert_eq!(
        changed
            .register_samples
            .iter()
            .find(|s| s.id == "private")
            .unwrap()
            .state,
        debugtui::registers::State::Valid
    );
    assert_eq!(
        changed
            .register_samples
            .iter()
            .find(|s| s.id == "cluster")
            .unwrap()
            .state,
        debugtui::registers::State::Stale
    );
    assert_eq!(reads(&out), 2);
    let legacy = changed.registers.iter().find(|r| r.name == "r0").unwrap();
    assert!(
        legacy.error,
        "the first invalidation snapshot must also expire the legacy projection"
    );
    assert_eq!(legacy.value, "0x01020304");
    assert_eq!(
        fs::read_to_string(out.join("commands.txt"))
            .unwrap()
            .lines()
            .filter(|c| c.starts_with("-data-list-register-values r "))
            .count(),
        1,
        "invalidating a legacy projection must not read the target again"
    );
    call(&engine, 3, "quit", json!({}));
}
