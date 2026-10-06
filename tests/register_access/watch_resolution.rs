use super::*;

fn fixture_project(name: &str, extra: &[(&str, &str)], watches: Vec<String>) -> (Project, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = root
        .join("artifacts")
        .join(format!("watch resolution {} {name}", std::process::id()));
    fs::create_dir_all(&output).unwrap();
    let transcript = output.join("commands.txt");
    let context = output.join("context.json");
    let elf = output.join("fixture.exe");
    fs::write(
        &context,
        json!({"thread":"1","frame":0,"pc":"0x100000008"}).to_string(),
    )
    .unwrap();
    // Only the PE byte-order header is needed by this MI fixture. The separate
    // native suite compiles and loads an actual executable with real DWARF.
    fs::write(&elf, b"MZ\0\0\0\0").unwrap();
    let mut project = Project::default();
    project.watch = watches;
    project.program.elf = elf;
    project.gdb.executable = "node".into();
    project.gdb.args = vec![
        root.join("tests/mock-gdb.cjs")
            .to_string_lossy()
            .into_owned(),
    ];
    for (key, value) in [
        ("DEBUGTUI_TEST_REGISTERS", "[]".to_owned()),
        ("DEBUGTUI_TEST_WATCH_RESOLVE", "1".to_owned()),
        (
            "DEBUGTUI_TEST_TRANSCRIPT",
            transcript.to_string_lossy().into_owned(),
        ),
        (
            "DEBUGTUI_TEST_CONTEXT_FILE",
            context.to_string_lossy().into_owned(),
        ),
        (
            "DEBUGTUI_TEST_WATCH_POLICY_FILE",
            output.join("policy.txt").to_string_lossy().into_owned(),
        ),
    ] {
        project.gdb.env.insert(key.into(), value);
    }
    for &(key, value) in extra {
        project.gdb.env.insert(key.into(), value.into());
    }
    project.target.endpoint = "localhost:3333".into();
    project.session.on_exit = "disconnect".into();
    (project, transcript)
}

fn fixture(
    name: &str,
    extra: &[(&str, &str)],
    watches: Vec<String>,
) -> (session::EngineHandle, PathBuf) {
    let (project, transcript) = fixture_project(name, extra, watches);
    let engine = session::spawn(project);
    response(&engine, 1, "connect", json!({}));
    fs::write(&transcript, "").unwrap();
    (engine, transcript)
}

#[test]
fn multicore_watch_proof_and_cancellation_stay_on_the_selected_worker_under_scope_all() {
    let (mut project, transcript) = fixture_project(
        "four-core",
        &[(
            "DEBUGTUI_TEST_MEMORY_BLOCKS",
            r#"[{begin="0x100000004",contents="11000000"}]"#,
        )],
        vec!["counter".into()],
    );
    project.cores = (0..4)
        .map(|index| debugtui::config::Core {
            name: format!("core{index}"),
            endpoint: format!("localhost:{}", 3333 + index),
            ..Default::default()
        })
        .collect();
    let engine = debugtui::coordinator::spawn(project);
    response(&engine, 1, "connect", json!({}));
    response(&engine, 2, "control_scope", json!({"scope":"all"}));
    fs::write(&transcript, "").unwrap();
    let mut first = json!(null);
    let mut first_binding = json!(null);
    for index in [0, 1, 2, 3] {
        response(
            &engine,
            10 + index * 10,
            "select_core",
            json!({"index":index}),
        );
        let listed = response(&engine, 11 + index * 10, "registers_list", json!({}));
        if index == 0 {
            first = listed["context"].clone();
        } else {
            assert!(
                request_error(
                    &engine,
                    12 + index * 10,
                    "watch_resolve",
                    json!({"expression":"counter","context":first})
                )
                .contains("expired core")
            );
        }
        let binding = response(
            &engine,
            13 + index * 10,
            "watch_resolve",
            json!({"expression":"counter","context":listed["context"]}),
        );
        assert_eq!(binding["context"], listed["context"]);
        assert_eq!(binding["context"]["core"], format!("core{index}"));
        if index == 0 {
            first_binding = binding.clone();
        } else {
            let mut wrong_worker = binding_params(&binding);
            wrong_worker["watch_binding"] = first_binding["binding_id"].clone();
            assert!(
                request_error(&engine, 14 + index * 10, "memory_read", wrong_worker)
                    .contains("Watch binding expired")
            );
        }
        let read = response(
            &engine,
            15 + index * 10,
            "memory_read",
            binding_params(&binding),
        );
        assert_eq!(read["value"], 17);
        assert_eq!(read["access"]["context"], listed["context"]);
        assert_eq!(
            read["access"]["route"]["configured_endpoint"],
            format!("localhost:{}", 3333 + index)
        );
    }
    let before_cancel = fs::read_to_string(&transcript).unwrap();
    assert_eq!(
        before_cancel
            .lines()
            .filter(|line| line.starts_with("-var-create"))
            .count(),
        4
    );
    for (id, method) in [
        (70, "watch_resolve"),
        (71, "memory_read"),
        (72, "memory_dump"),
        (73, "peripheral_read"),
    ] {
        let request = Request::new(
            id,
            method,
            json!({"expression":"counter","address":0x100000004u64,"bits":32,"count":4,"little_endian":true}),
        );
        request.cancel_read();
        engine.send(request).unwrap();
        loop {
            if let Event::Response {
                id: found,
                ok,
                result,
                error,
            } = engine.events.recv_timeout(Duration::from_secs(10)).unwrap()
                && found == id
            {
                assert!(!ok && error.unwrap().contains("cancelled") && result.is_null());
                break;
            }
        }
    }
    let commands = fs::read_to_string(&transcript).unwrap();
    assert_eq!(
        commands, before_cancel,
        "Cancelled reads must reach the selected worker without transport"
    );
    no_target_changes(&commands);
    response(&engine, 99, "quit", json!({}));
}

fn request_error(engine: &session::EngineHandle, id: u64, method: &str, params: Value) -> String {
    engine.send(Request::new(id, method, params)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
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
            assert!(!ok, "{method}: unexpected result {result}");
            assert!(
                result.is_null(),
                "Failed Watch must not publish a binding: {result}"
            );
            return error.unwrap();
        }
    }
}
fn binding_params(binding: &Value) -> Value {
    json!({"address":binding["address"],"bits":binding["bits"],"little_endian":binding["little_endian"],
        "context":binding["context"],"selection_epoch":binding["selection_epoch"],"watch_binding":binding["binding_id"],"channel":""})
}

#[test]
fn watch_binding_detects_silent_thread_frame_and_pc_changes_before_memory_dispatch() {
    for scenario in ["thread", "frame", "pc"] {
        let (engine, transcript) = fixture(
            &format!("binding-{scenario}"),
            &[(
                "DEBUGTUI_TEST_MEMORY_BLOCKS",
                r#"[{begin="0x100000004",contents="11000000"}]"#,
            )],
            vec!["counter".into()],
        );
        let binding = response(&engine, 2, "watch_resolve", json!({"expression":"counter"}));
        assert!(!binding["binding_id"].as_str().unwrap().is_empty());
        let read = response(&engine, 3, "memory_read", binding_params(&binding));
        assert_eq!(read["value"], 17);
        let mut actual = json!({"thread":"1","frame":0,"pc":"0x100000008"});
        match scenario {
            "thread" => actual["thread"] = json!("2"),
            "frame" => actual["frame"] = json!(1),
            "pc" => actual["pc"] = json!("0x10000000c"),
            _ => unreachable!(),
        }
        let context_path = transcript.parent().unwrap().join("context.json");
        fs::write(&context_path, actual.to_string()).unwrap();
        fs::write(&transcript, "").unwrap();
        assert!(
            request_error(&engine, 4, "memory_read", binding_params(&binding))
                .contains("different selected thread, frame or PC")
        );
        let commands = fs::read_to_string(&transcript).unwrap();
        assert!(commands.contains("-thread-info") && commands.contains("-stack-info-frame"));
        assert!(!commands.contains("-data-read-memory-bytes"));
        no_target_changes(&commands);
        let status = response(&engine, 5, "status", json!({}));
        assert_eq!(
            status["memory_selection_epoch"].as_u64().unwrap(),
            binding["selection_epoch"].as_u64().unwrap() + 1
        );
        fs::write(&transcript, "").unwrap();
        let expected = if scenario == "frame" {
            "expired core, frame or stop context"
        } else {
            "expired thread selection"
        };
        assert!(
            request_error(&engine, 6, "memory_read", binding_params(&binding)).contains(expected)
        );
        assert!(fs::read_to_string(&transcript).unwrap().is_empty());
        fs::write(
            &context_path,
            json!({"thread":"1","frame":0,"pc":"0x100000008"}).to_string(),
        )
        .unwrap();
        response(&engine, 7, "refresh", json!({}));
        let fresh = response(&engine, 8, "watch_resolve", json!({"expression":"counter"}));
        assert_ne!(fresh["binding_id"], binding["binding_id"]);
        assert_eq!(
            response(&engine, 9, "memory_read", binding_params(&fresh))["value"],
            17
        );
        response(&engine, 99, "quit", json!({}));
    }
}

#[test]
fn watch_binding_rejects_modified_type_and_expires_on_resolve_frame_and_reconnect() {
    let (engine, transcript) = fixture(
        "binding-lifetime",
        &[(
            "DEBUGTUI_TEST_MEMORY_BLOCKS",
            r#"[{begin="0x100000004",contents="11000000"}]"#,
        )],
        vec!["counter".into()],
    );
    let binding = response(&engine, 2, "watch_resolve", json!({"expression":"counter"}));
    fs::write(&transcript, "").unwrap();
    for (index, scenario) in [
        "token-null",
        "token-empty",
        "address",
        "bits",
        "endian",
        "epoch",
    ]
    .iter()
    .enumerate()
    {
        let mut bad = binding_params(&binding);
        match *scenario {
            "token-null" => bad["watch_binding"] = Value::Null,
            "token-empty" => bad["watch_binding"] = json!(""),
            "address" => bad["address"] = json!(0x100000008u64),
            "bits" => bad["bits"] = json!(64),
            "endian" => bad["little_endian"] = json!(false),
            "epoch" => bad["selection_epoch"] = json!(999),
            _ => unreachable!(),
        }
        request_error(&engine, 10 + index as u64, "memory_read", bad);
        assert!(
            fs::read_to_string(&transcript).unwrap().is_empty(),
            "{scenario}"
        );
    }
    let fresh = response(
        &engine,
        20,
        "watch_resolve",
        json!({"expression":"counter"}),
    );
    assert_ne!(fresh["binding_id"], binding["binding_id"]);
    fs::write(&transcript, "").unwrap();
    assert!(
        request_error(&engine, 21, "memory_read", binding_params(&binding))
            .contains("Watch binding expired")
    );
    assert!(fs::read_to_string(&transcript).unwrap().is_empty());
    response(&engine, 22, "frame", json!({"level":0}));
    fs::write(&transcript, "").unwrap();
    assert!(
        request_error(&engine, 23, "memory_read", binding_params(&fresh))
            .contains("expired thread selection")
    );
    assert!(fs::read_to_string(&transcript).unwrap().is_empty());
    let before_reconnect = response(
        &engine,
        24,
        "watch_resolve",
        json!({"expression":"counter"}),
    );
    response(&engine, 25, "disconnect", json!({}));
    response(&engine, 26, "connect", json!({}));
    fs::write(&transcript, "").unwrap();
    assert!(
        request_error(
            &engine,
            27,
            "memory_read",
            binding_params(&before_reconnect)
        )
        .contains("expired core")
    );
    assert!(fs::read_to_string(&transcript).unwrap().is_empty());
    let fresh = response(
        &engine,
        28,
        "watch_resolve",
        json!({"expression":"counter"}),
    );
    assert_ne!(
        fresh["context"]["session"],
        before_reconnect["context"]["session"]
    );
    assert_eq!(
        response(&engine, 29, "memory_read", binding_params(&fresh))["value"],
        17
    );
    response(&engine, 99, "quit", json!({}));
}

fn no_target_changes(commands: &str) {
    for command in commands.lines() {
        assert!(
            !command.starts_with("-exec-")
                && !command.starts_with("-target-select")
                && !command.starts_with("-data-write-")
                && !command.starts_with("-var-assign"),
            "{command}"
        );
    }
}

#[test]
fn watch_resolution_rejects_unsafe_expressions_expired_context_and_unbounded_children_before_mi() {
    let expressions = [
        "counter++",
        "func()",
        "array[next()]",
        "counter=1",
        "$pc",
        "delete *ptr",
    ];
    let (engine, transcript) = fixture(
        "preflight",
        &[],
        expressions
            .iter()
            .map(|s| (*s).into())
            .chain(["counter".into()])
            .collect(),
    );
    let listed = response(&engine, 2, "registers_list", json!({}));
    fs::write(&transcript, "").unwrap();
    for (i, expression) in expressions.iter().enumerate() {
        let error = request_error(
            &engine,
            i as u64 + 10,
            "watch_resolve",
            json!({"expression":expression,"context":listed["context"]}),
        );
        assert!(error.contains("read-only C/C++"), "{error}");
    }
    let mut expired = listed["context"].clone();
    expired["core"] = json!("another-core");
    assert!(
        request_error(
            &engine,
            30,
            "watch_resolve",
            json!({"expression":"counter","context":expired})
        )
        .contains("expired core")
    );
    assert!(
        request_error(
            &engine,
            31,
            "watch_resolve",
            json!({"expression":"counter","path":[4096]})
        )
        .contains("exceeds its bounds")
    );
    assert!(
        request_error(
            &engine,
            32,
            "watch_resolve",
            json!({"expression":"counter","path":[0,0,0,0,0,0,0,0,0]})
        )
        .contains("exceeds its bounds")
    );
    assert!(
        fs::read_to_string(&transcript).unwrap().is_empty(),
        "Preflight rejection must send zero MI commands"
    );
    response(&engine, 99, "quit", json!({}));
}

#[test]
fn watch_resolution_proves_stopped_thread_frame_and_restores_the_original_call_policy() {
    for off in [false, true] {
        let extra = if off {
            vec![("DEBUGTUI_TEST_WATCH_CALLS_OFF", "1")]
        } else {
            vec![]
        };
        let (engine, transcript) = fixture(
            if off { "calls-off" } else { "calls-on" },
            &extra,
            vec!["counter".into()],
        );
        let listed = response(&engine, 2, "registers_list", json!({}));
        let result = response(
            &engine,
            3,
            "watch_resolve",
            json!({"expression":"counter","context":listed["context"]}),
        );
        assert_eq!(result["address"], 0x100000004u64);
        assert_eq!(result["bits"], 32);
        assert_eq!(result["context"], listed["context"]);
        assert_eq!(result["thread"], "1");
        assert_eq!(result["frame_address"], "0x100000008");
        assert_eq!(result["source"], "gdb_typed_address");
        assert_eq!(result["state"], "STOPPED");
        let commands = fs::read_to_string(&transcript).unwrap();
        assert_eq!(commands.lines().filter(|s| *s == "-thread-info").count(), 2);
        assert_eq!(
            commands
                .lines()
                .filter(|s| *s == "-stack-info-frame")
                .count(),
            2
        );
        let create = commands.find("-var-create - * \"counter\"").unwrap();
        let delete = commands.find("-var-delete").unwrap();
        if off {
            assert!(!commands.contains("-gdb-set may-call-functions"));
        } else {
            assert!(commands.find("-gdb-set may-call-functions off").unwrap() < create);
            assert!(delete < commands.find("-gdb-set may-call-functions on").unwrap());
        }
        no_target_changes(&commands);
        assert_eq!(
            fs::read_to_string(transcript.parent().unwrap().join("policy.txt")).unwrap(),
            if off { "off" } else { "on" }
        );
        response(&engine, 99, "quit", json!({}));
    }
}

#[test]
fn watch_resolution_discards_silent_thread_frame_pc_changes_and_mid_operation_notices() {
    for (scenario, extra, expected) in [
        (
            "thread",
            vec![("DEBUGTUI_TEST_WATCH_CHANGE", "thread")],
            "Selected GDB thread",
        ),
        (
            "frame",
            vec![("DEBUGTUI_TEST_WATCH_CHANGE", "frame")],
            "Selected GDB thread",
        ),
        (
            "pc",
            vec![("DEBUGTUI_TEST_WATCH_CHANGE", "pc")],
            "Selected GDB thread",
        ),
        (
            "cleanup-thread",
            vec![
                ("DEBUGTUI_TEST_WATCH_CHANGE", "thread"),
                ("DEBUGTUI_TEST_WATCH_CHANGE_AT", "cleanup"),
            ],
            "Selected GDB thread",
        ),
        (
            "running",
            vec![("DEBUGTUI_TEST_WATCH_RUNNING", "1")],
            "running state changed",
        ),
        (
            "stop-again",
            vec![("DEBUGTUI_TEST_WATCH_STOP_AGAIN", "1")],
            "running state changed",
        ),
        (
            "thread-notice",
            vec![("DEBUGTUI_TEST_WATCH_THREAD_NOTICE", "1")],
            "running state changed",
        ),
    ] {
        let (engine, transcript) = fixture(scenario, &extra, vec!["counter".into()]);
        let error = request_error(&engine, 3, "watch_resolve", json!({"expression":"counter"}));
        assert!(error.contains(expected), "{scenario}: {error}");
        let commands = fs::read_to_string(&transcript).unwrap();
        assert!(
            commands.contains("-var-delete") && commands.contains("-gdb-set may-call-functions on")
        );
        if matches!(scenario, "running" | "stop-again" | "thread-notice") {
            assert!(
                !commands.contains("sizeof(counter)"),
                "No more target expression probes after the notice"
            );
        }
        no_target_changes(&commands);
        response(&engine, 99, "quit", json!({}));
    }
}

#[test]
fn watch_resolution_never_ignores_cleanup_restore_or_resolved_path_failures() {
    for (scenario, extra, expected, fault) in [
        (
            "cleanup-error",
            vec![("DEBUGTUI_TEST_WATCH_CLEANUP_ERROR", "1")],
            "Watch object cleanup failed",
            true,
        ),
        (
            "zero-deleted",
            vec![("DEBUGTUI_TEST_WATCH_ZERO_DELETED", "1")],
            "Watch object cleanup failed",
            true,
        ),
        (
            "restore-error",
            vec![("DEBUGTUI_TEST_WATCH_RESTORE_ERROR", "1")],
            "policy restoration failed",
            true,
        ),
        (
            "resolved-call",
            vec![("DEBUGTUI_TEST_WATCH_PATH", "next()")],
            "read-only C/C++",
            false,
        ),
        (
            "resolved-increment",
            vec![("DEBUGTUI_TEST_WATCH_PATH", "counter++")],
            "read-only C/C++",
            false,
        ),
        (
            "no-address",
            vec![("DEBUGTUI_TEST_WATCH_NO_ADDRESS", "1")],
            "without an address",
            false,
        ),
        (
            "aggregate",
            vec![("DEBUGTUI_TEST_WATCH_AGGREGATE", "1")],
            "Expand the aggregate",
            false,
        ),
    ] {
        let (engine, transcript) = fixture(scenario, &extra, vec!["counter".into()]);
        let error = request_error(&engine, 3, "watch_resolve", json!({"expression":"counter"}));
        assert!(error.contains(expected), "{scenario}: {error}");
        let status = response(&engine, 4, "status", json!({}));
        assert_eq!(status["state"], if fault { "FAULT" } else { "STOPPED" });
        let commands = fs::read_to_string(&transcript).unwrap();
        assert!(
            commands.contains("-var-delete") && commands.contains("-gdb-set may-call-functions on")
        );
        if scenario.starts_with("resolved-") {
            assert!(!commands.contains("-data-evaluate-expression"));
        }
        no_target_changes(&commands);
        response(&engine, 99, "quit", json!({}));
    }
}

#[test]
fn malformed_memory_channel_never_silently_selects_gdb() {
    let (engine, transcript) = fixture("bad-channel", &[], vec![]);
    for (i, method) in ["memory_read", "memory_dump", "peripheral_read"]
        .iter()
        .enumerate()
    {
        for (j, channel) in [json!(null), json!(1), json!(false), json!({}), json!([])]
            .into_iter()
            .enumerate()
        {
            let address = if *method == "memory_dump" {
                json!("0x100000004")
            } else {
                json!(0x100000004u64)
            };
            let error = request_error(
                &engine,
                10 + i as u64 * 10 + j as u64,
                method,
                json!({"channel":channel,"address":address,"bits":32,"count":4,"little_endian":true}),
            );
            assert!(
                error.contains("channel must be a string"),
                "{method}: {error}"
            );
        }
    }
    assert!(fs::read_to_string(&transcript).unwrap().is_empty());
    response(&engine, 99, "quit", json!({}));
}

#[test]
fn retained_watch_and_memory_request_cancellation_reaches_the_actual_worker() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let ready = root
        .join("artifacts")
        .join(format!("watch cancel ready {}.txt", std::process::id()));
    let release = root
        .join("artifacts")
        .join(format!("watch cancel release {}.txt", std::process::id()));
    let ready_text = ready.to_string_lossy().into_owned();
    let release_text = release.to_string_lossy().into_owned();
    let (engine, transcript) = fixture(
        "cancel",
        &[
            ("DEBUGTUI_TEST_WATCH_READY_FILE", &ready_text),
            ("DEBUGTUI_TEST_WATCH_RELEASE_FILE", &release_text),
        ],
        vec!["counter".into()],
    );
    // The unique process paths need no cleanup; do not reuse a previous signal.
    assert!(!ready.exists() && !release.exists());
    for (id, method) in [
        (3, "watch_resolve"),
        (4, "memory_read"),
        (5, "memory_dump"),
        (6, "peripheral_read"),
    ] {
        let request = Request::new(
            id,
            method,
            json!({"expression":"counter","address":0x100000004u64,"bits":32,"count":4,"little_endian":true}),
        );
        request.cancel_read();
        engine.send(request).unwrap();
        loop {
            if let Event::Response {
                id: found,
                ok,
                error,
                result,
            } = engine.events.recv_timeout(Duration::from_secs(10)).unwrap()
                && found == id
            {
                assert!(!ok && error.unwrap().contains("cancelled") && result.is_null());
                break;
            }
        }
    }
    assert!(fs::read_to_string(&transcript).unwrap().is_empty());
    let request = Request::new(7, "watch_resolve", json!({"expression":"counter"}));
    engine.send(request.clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() {
        assert!(
            Instant::now() < deadline,
            "Watch address dispatch was not reached"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    request.cancel_read();
    fs::write(&release, "cancel delivered; return the MI response").unwrap();
    loop {
        if let Event::Response {
            id: 7,
            ok,
            error,
            result,
        } = engine.events.recv_timeout(Duration::from_secs(10)).unwrap()
        {
            assert!(!ok && error.unwrap().contains("cancelled") && result.is_null());
            break;
        }
    }
    let commands = fs::read_to_string(&transcript).unwrap();
    assert!(
        commands.contains("&(counter)")
            && commands.contains("-var-delete")
            && commands.contains("-gdb-set may-call-functions on")
    );
    assert!(!commands.contains("sizeof(counter)"));
    assert_eq!(
        fs::read_to_string(transcript.parent().unwrap().join("policy.txt")).unwrap(),
        "on"
    );
    no_target_changes(&commands);
    assert_eq!(
        response(&engine, 8, "status", json!({}))["state"],
        "STOPPED"
    );
    // Cancellation is per request, so the next scoped read is still allowed.
    assert_eq!(
        response(&engine, 9, "watch_resolve", json!({"expression":"counter"}))["address"],
        0x100000004u64
    );
    response(&engine, 99, "quit", json!({}));
}
