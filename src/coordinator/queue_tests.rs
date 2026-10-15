use super::*;

/// No producer is left to ring after the queued requests: processing them must
/// not depend on another keystroke, GDB log line or worker timeout.
fn queued_requests_finish_without_another_wake(completed_worker: bool) {
    let project = Project {
        cores: vec![Core {
            name: "core.0".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let (events, receiver) = mpsc::sync_channel(512);
    let cancellation = Arc::new(AtomicBool::new(false));
    let inbox = Doorbell::default();
    let (commands, requests) = mpsc::channel();
    let handle = EngineHandle {
        commands,
        events: receiver,
        cancellation: cancellation.clone(),
        bell: RingOnDrop(inbox.clone()),
    };
    let (ready, initialized) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let mut coordinator =
            Coordinator::new_with(project, events, cancellation, inbox, Doorbell::default());
        let preferences = Request::new(
            1,
            method::REGISTER_PREFERENCES,
            json!({"scope":"rapid-filter", "preferences":{"query":"NVIC.ISER0"}}),
        );
        if completed_worker {
            coordinator.queue.push_back(preferences);
            coordinator.advance();
            // Stop immediately after the worker's response has arrived, before
            // advance() replies to the client and finishes that batch.
            loop {
                let event = coordinator.engines[0]
                    .handle
                    .events
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
                let response = matches!(event, Event::Response { .. });
                coordinator.event(0, event);
                if response {
                    break;
                }
            }
            assert!(coordinator.batch.as_ref().unwrap().waiting.is_none());
        } else {
            // CORES replies synchronously inside begin(), without a worker
            // response that might accidentally wake the next queued request.
            coordinator
                .queue
                .push_back(Request::new(0, method::CORES, json!({})));
            coordinator.queue.push_back(preferences);
        }
        coordinator.queue.push_back(Request::new(
            2,
            method::REGISTERS_READ,
            json!({"ids":["nvic.iser0"]}),
        ));
        coordinator
            .queue
            .push_back(Request::new(3, method::STATUS, json!({})));
        // All earlier notifications were coalesced and consumed. A queued read
        // on this disconnected fixture must promptly return its normal error.
        while coordinator.inbox.wait(Some(Instant::now())) {}
        ready.send(()).unwrap();
        coordinator.run(requests);
    });
    initialized.recv_timeout(Duration::from_secs(5)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut responses = Vec::new();
    while let Ok(event) = handle
        .events
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
    {
        if let Event::Response { id, ok, error, .. } = event {
            responses.push((id, ok, error));
            if id == 3 {
                break;
            }
        }
    }
    // Always wake and close our workers before asserting, including on the
    // unfixed implementation; a failing regression must not leak threads.
    handle
        .send(Request::new(99, method::QUIT, json!({})))
        .unwrap();
    let close_deadline = Instant::now() + Duration::from_secs(5);
    let mut exited = false;
    while Instant::now() < close_deadline {
        match handle.events.recv_timeout(Duration::from_millis(50)) {
            Ok(Event::Exit) => {
                exited = true;
                break;
            }
            Ok(_) => {}
            // Rescue the old scheduler during cleanup only, after the
            // response deadline and without altering the captured result.
            Err(mpsc::RecvTimeoutError::Timeout) => handle.bell.0.ring(),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    assert!(exited, "Coordinator did not close");
    worker.join().unwrap();
    let expected = if completed_worker {
        vec![1, 2, 3]
    } else {
        vec![0, 1, 2, 3]
    };
    assert_eq!(
        responses.iter().map(|r| r.0).collect::<Vec<_>>(),
        expected,
        "Queued work stalled after {}",
        if completed_worker {
            "worker response"
        } else {
            "immediate response"
        }
    );
    let read = responses.iter().find(|r| r.0 == 2).unwrap();
    assert!(!read.1);
    assert!(
        read.2
            .as_ref()
            .unwrap()
            .contains("Register reads require a connected")
    );
    assert!(responses.iter().filter(|r| r.0 != 2).all(|r| r.1));
}

#[test]
fn queued_filter_read_runs_after_worker_response_without_another_wake() {
    queued_requests_finish_without_another_wake(true);
}

#[test]
fn queued_filter_read_runs_after_immediate_response_without_another_wake() {
    queued_requests_finish_without_another_wake(false);
}
