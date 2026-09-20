use super::*;
use crate::live_watch::LiveWatchSample;

fn sample(value: u64, core: Option<usize>, generation: u64) -> Event {
    Event::LiveWatch {
        sample: LiveWatchSample {
            expression: "uart_cnt".into(),
            address: 0x100209bc,
            bits: 32,
            value: Some(value),
            error: None,
            timestamp: "2026-09-20 16:43:37.659".into(),
            core,
            generation,
        },
    }
}
fn app() -> App {
    let mut a = App::new(Project::default(), false);
    a.snapshot.state = "RUNNING".into();
    a.snapshot.generation = 7;
    a.snapshot.watches = vec![Variable {
        name: "uart_cnt".into(),
        value: "5781".into(),
        ..Default::default()
    }];
    a
}
fn watch_line(a: &mut App) -> String {
    let mut t = Terminal::new(TestBackend::new(160, 45)).unwrap();
    t.draw(|f| draw(f, a)).unwrap();
    let hit = a
        .formats
        .hits
        .iter()
        .find(|v| v.pane == 1 && v.name == "uart_cnt" && v.row % 2 == 1)
        .unwrap()
        .rect;
    (hit.x..hit.right())
        .map(|x| t.backend().buffer()[(x, hit.y)].symbol())
        .collect()
}
#[test]
fn live_events_change_rendered_watch_without_console_flood_or_snapshot_corruption() {
    let mut a = app();
    let logs = a.console.len();
    a.update(sample(7355, None, 7));
    let first = watch_line(&mut a);
    assert!(first.contains("7355 <raw 32-bit>"), "{first}");
    assert!(first.contains("LIVE"));
    a.update(sample(7500, None, 7));
    let next = watch_line(&mut a);
    assert!(next.contains("7500 <raw 32-bit>"));
    assert!(!next.contains("5781"));
    assert_eq!(a.snapshot.watches[0].value, "5781");
    assert_eq!(a.console.len(), logs);
    a.update(Event::Snapshot {
        snapshot: Box::new(a.snapshot.clone()),
    });
    assert!(watch_line(&mut a).contains("7500"));
    a.project
        .ui
        .formats
        .insert("watch:uart_cnt".into(), crate::config::Radix::Hex);
    assert!(watch_line(&mut a).contains("0x1d4c <raw 32-bit>"));
    a.update(sample(7500, None, 7));
    assert!(!a.watch_sample("watch:uart_cnt").unwrap().1);
    assert!(a.monitor_fresh("watch:uart_cnt"));
}
#[test]
fn late_wrong_core_deleted_and_stopped_samples_are_ignored() {
    let mut a = app();
    a.update(sample(1, Some(1), 7));
    a.update(sample(2, None, 6));
    assert!(a.watch_sample("watch:uart_cnt").is_none());
    a.update(sample(7500, None, 7));
    let mut stopped = a.snapshot.clone();
    stopped.state = "STOPPED".into();
    stopped.generation = 8;
    stopped.watches[0].value = "7600".into();
    a.update(Event::Snapshot {
        snapshot: Box::new(stopped),
    });
    a.update(sample(7501, None, 7));
    assert!(a.watch_sample("watch:uart_cnt").is_none());
    let line = watch_line(&mut a);
    assert!(line.contains("7600"));
    assert!(!line.contains("LIVE"));
    a.snapshot.state = "RUNNING".into();
    a.snapshot.watches.clear();
    a.update(sample(9999, None, 8));
    assert!(a.watch_sample("watch:uart_cnt").is_none());
}
#[test]
fn read_failures_clear_live_badge_and_explicit_policies_take_precedence() {
    let mut a = app();
    a.update(sample(3, None, 7));
    let Event::LiveWatch { mut sample } = sample(3, None, 7) else {
        unreachable!()
    };
    sample.value = None;
    sample.error = Some("TCL connection closed".into());
    a.update(Event::LiveWatch { sample });
    assert!(!a.monitor_fresh("watch:uart_cnt"));
    assert!(watch_line(&mut a).contains("TCL connection closed"));
    a.update(super::live_watch_tests::sample(4, None, 7));
    assert!(a.monitor_fresh("watch:uart_cnt"));
    a.project
        .ui
        .refresh
        .insert("single|watch:uart_cnt".into(), Default::default());
    a.monitor.invalidate();
    a.update(super::live_watch_tests::sample(5, None, 7));
    assert!(a.watch_sample("watch:uart_cnt").is_none());
}
/// Replay actual hardware events through the real App and Watch renderer.
#[test]
fn replay_hardware_live_watch_events_when_requested() {
    let Ok(events) = std::env::var("DEBUGTUI_LIVE_EVENTS") else {
        return;
    };
    let project =
        Project::load(Path::new(&std::env::var("DEBUGTUI_LIVE_PROJECT").unwrap())).unwrap();
    let mut a = App::new(project, false);
    let mut seen = std::collections::HashMap::<usize, Vec<u64>>::new();
    for line in fs::read_to_string(events).unwrap().lines() {
        let e: Event = serde_json::from_str(line).unwrap();
        let live = if let Event::LiveWatch { sample } = &e {
            sample
                .value
                .filter(|_| sample.expression == "uart_cnt")
                .map(|v| (sample.core.unwrap_or(0), v))
        } else {
            None
        };
        a.update(e);
        if let Some((core, value)) = live {
            let line = watch_line(&mut a);
            assert!(line.contains(&format!("{value} <raw 32-bit>")), "{line}");
            assert!(line.contains("LIVE"));
            let values = seen.entry(core).or_default();
            if let Ok(root) = std::env::var("DEBUGTUI_RENDER_DIR") {
                fs::create_dir_all(&root).unwrap();
                if values.is_empty() {
                    visual_tests::capture(
                        Path::new(&root),
                        &format!("live-core{core}-first"),
                        &mut a,
                        160,
                        45,
                    );
                }
                visual_tests::capture(
                    Path::new(&root),
                    &format!("live-core{core}-latest"),
                    &mut a,
                    160,
                    45,
                );
            }
            values.push(value);
        }
    }
    for core in [0, 1] {
        let values = &seen[&core];
        assert!(values.len() >= 3 && values.windows(2).any(|v| v[0] != v[1]));
    }
}
