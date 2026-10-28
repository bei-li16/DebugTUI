use super::*;
#[test]
fn cancellation_without_a_running_backend_deletes_only_its_own_breakpoint_without_halt() {
    let mut command = Command::new("node");
    command.args(["-e", "process.stdin.pipe(process.stdout)"]);
    command.stdin(Stdio::piped()).stdout(Stdio::piped());
    hidden(&mut command);
    let mut child = command.spawn().unwrap();
    let input = child.stdin.take().unwrap();
    let mut echo = BufReader::new(child.stdout.take().unwrap());
    let (tx, records) = mpsc::channel();
    // Reply only after each request. Prequeuing future replies incorrectly lets
    // the pre-cancellation notice drain consume responses to commands not sent.
    let responder = thread::spawn(move || {
        let mut commands = String::new();
        for response in [
            "1^done,BreakpointTable={body=[bkpt={number=\"3\"},bkpt={number=\"7\"}]}",
            "2^done",
        ] {
            echo.read_line(&mut commands).unwrap();
            tx.send(Incoming::Record(mi::parse(response).unwrap().unwrap()))
                .unwrap();
        }
        commands
    });
    let (events, _rx) = mpsc::sync_channel(64);
    let mut engine = Engine::new(Project::default(), events, Arc::new(AtomicBool::new(false)));
    engine.gdb = Some(Gdb {
        child,
        input,
        records,
        token: 0,
    });
    engine.snapshot.state = state::RUNNING.into();
    engine.snapshot.image_state = Some(Default::default());
    engine.guest_return = Some(ReturnOperation {
        id: "7".into(),
        pc: 0x10020004,
        image: "vm0".into(),
        hit: false,
        failure: None,
        running_route: None,
        cancelled: false,
        cancellation: None,
    });
    let result = engine.cancel_guest_return().unwrap();
    assert_eq!(result["target_control"], false);
    assert_eq!(engine.snapshot.state, state::RUNNING);
    assert!(engine.guest_return.is_none());
    let commands = responder.join().unwrap();
    assert_eq!(commands, "1-break-list\n2-break-delete 7\n");
    assert_eq!(
        engine.snapshot.image_state.unwrap().return_guest.unwrap()["state"],
        "cancelled"
    );
}

#[test]
fn asynchronous_resource_failure_needs_a_confirmed_stop_and_deletes_only_its_own_breakpoint() {
    let mut command = Command::new("node");
    command.args(["-e", "process.stdin.pipe(process.stdout)"]);
    command.stdin(Stdio::piped()).stdout(Stdio::piped());
    hidden(&mut command);
    let mut child = command.spawn().unwrap();
    let input = child.stdin.take().unwrap();
    let mut echo = BufReader::new(child.stdout.take().unwrap());
    let (tx, records) = mpsc::channel();
    for line in [
        "1^done,threads=[{id=\"1\",state=\"stopped\"}],current-thread-id=\"1\"",
        "2^done,BreakpointTable={body=[bkpt={number=\"3\"},bkpt={number=\"7\"}]}",
        "3^done",
    ] {
        tx.send(Incoming::Record(mi::parse(line).unwrap().unwrap()))
            .unwrap();
    }
    let (events, _rx) = mpsc::sync_channel(64);
    let mut engine = Engine::new(Project::default(), events, Arc::new(AtomicBool::new(false)));
    engine.gdb = Some(Gdb {
        child,
        input,
        records,
        token: 0,
    });
    engine.snapshot.state = state::RUNNING.into();
    engine.snapshot.image_state = Some(Default::default());
    engine.guest_return = Some(ReturnOperation {
        id: "7".into(),
        pc: 0x10020004,
        image: "vm0".into(),
        hit: false,
        failure: None,
        running_route: None,
        cancelled: false,
        cancellation: None,
    });
    engine.record(Incoming::Record(
        mi::parse("@\"Could not insert hardware breakpoints:\\n\"")
            .unwrap()
            .unwrap(),
    ));
    assert!(
        engine.guest_return.as_ref().unwrap().failure.is_none(),
        "Program output is not a backend error"
    );
    engine.record(Incoming::Record(
        mi::parse("&\"Cannot insert hardware breakpoint 7.\\n\"")
            .unwrap()
            .unwrap(),
    ));
    assert_eq!(
        engine.snapshot.state,
        state::RUNNING,
        "A diagnostic is not stop evidence"
    );
    assert!(engine.execution_failure.is_some());
    engine.reconcile_execution_failure().unwrap();
    assert_eq!(engine.snapshot.state, state::STOPPED);
    engine.finish_guest_return().unwrap();
    assert!(engine.guest_return.is_none());
    let result = engine
        .snapshot
        .image_state
        .as_ref()
        .unwrap()
        .return_guest
        .as_ref()
        .unwrap();
    assert_eq!(result["state"], "failed");
    assert!(
        result["detail"]
            .as_str()
            .unwrap()
            .contains("Cannot insert hardware breakpoint 7")
    );
    let mut commands = String::new();
    for _ in 0..3 {
        echo.read_line(&mut commands).unwrap();
    }
    assert_eq!(commands, "1-thread-info\n2-break-list\n3-break-delete 7\n");
}
