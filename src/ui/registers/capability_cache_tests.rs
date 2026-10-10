use super::tests::{engine, sample};
use super::*;
use crate::registers::{RawValue, Reader, SampleView};

// This exercises the real UI cache and renderer with responses captured by the
// opt-in STM32 driver, rather than claiming a headless backend proves UI state.
#[test]
#[ignore = "requires DEBUGTUI_REGISTER_UI_REPLAY from the STM32 hardware driver"]
fn stm32_board_capture_replays_valid_stale_refresh_and_rendering() {
    let file = std::env::var("DEBUGTUI_REGISTER_UI_REPLAY").expect("hardware capture path");
    let capture: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let project = Project::load(Path::new(capture["project"].as_str().unwrap())).unwrap();
    let mut app = App::new(project, false);
    app.select_pane(pane::REGS);
    let update = |app: &mut App, snapshot: &Value| {
        app.update(crate::session::Event::Snapshot {
            snapshot: Box::new(serde_json::from_value(snapshot.clone()).unwrap()),
        });
    };
    update(&mut app, &capture["initial"]);
    let receive = |app: &mut App, stage: &Value, id: u64| {
        app.register_view.pending = Some((id, app.register_context()));
        assert!(app.register_response(id, &stage["result"], None));
        update(app, &stage["status"]);
        let core = app.register_context().core;
        for value in stage["result"]["samples"].as_array().unwrap() {
            let name = value["id"].as_str().unwrap();
            let index = app
                .register_view
                .catalogue
                .as_ref()
                .unwrap()
                .registers
                .iter()
                .position(|r| r.id == name)
                .unwrap();
            let owner = app
                .register_view
                .owner(&app.project, &app.register_context(), index)
                .unwrap();
            let cached = &app.register_view.values[&(owner, name.into(), core.clone())];
            assert_eq!(cached.state, State::Valid, "{name}");
            assert_eq!(cached.value.as_ref().unwrap().hex, value["value"]["hex"]);
            assert_eq!(
                app.register_view
                    .category(&app.project, &app.register_context(), index, true),
                status::Category::Valid,
                "{name}"
            );
        }
    };
    let stages = capture["reads"].as_array().unwrap();
    assert!(
        stages.len() >= 4,
        "at least four independently captured read batches"
    );
    for (id, stage) in stages.iter().enumerate() {
        receive(&mut app, stage, id as u64 + 1);
        assert!(
            app.register_view
                .values
                .values()
                .all(|s| s.state == State::Valid),
            "false Stale after stage {id}"
        );
    }
    let render = |app: &mut App, name: &str| {
        app.register_view.query = name.into();
        app.register_view.rebuild();
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 30)).unwrap();
        terminal
            .draw(|f| app.draw_registers(f, Rect::new(0, 0, 120, 30)))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>()
    };
    let valid = render(&mut app, "mpu.type");
    assert!(valid.contains("0x00000800"), "{valid}");
    assert!(!valid.contains("[Stale]"), "{valid}");
    std::fs::write(
        Path::new(&file).with_file_name("ui-valid-render.txt"),
        &valid,
    )
    .unwrap();
    if let Some(names) = capture["render_registers"].as_array() {
        for name in names {
            let name = name.as_str().unwrap();
            let valid = render(&mut app, name);
            assert!(!valid.contains("[Stale]"), "{name}: {valid}");
            let label = &app
                .register_view
                .catalogue
                .as_ref()
                .unwrap()
                .register(name)
                .unwrap()
                .name;
            assert!(valid.contains(label), "{name}: {valid}");
            std::fs::write(
                Path::new(&file).with_file_name(format!("ui-module-{name}-valid.txt")),
                valid,
            )
            .unwrap();
        }
    }
    update(&mut app, &capture["stale"]);
    assert!(
        app.register_view
            .values
            .values()
            .any(|s| s.state == State::Stale)
    );
    let stale = render(&mut app, "mpu.type");
    assert!(stale.contains("[Stale]"), "{stale}");
    std::fs::write(
        Path::new(&file).with_file_name("ui-stale-render.txt"),
        &stale,
    )
    .unwrap();
    receive(&mut app, &capture["refreshed"], 100);
    let refreshed = render(&mut app, "mpu.type");
    assert!(!refreshed.contains("[Stale]"), "{refreshed}");
    std::fs::write(
        Path::new(&file).with_file_name("ui-refreshed-render.txt"),
        &refreshed,
    )
    .unwrap();
}

fn m4() -> App {
    let project = Project {
        registers: toml::from_str(
            "cpu='cortex-m4'\n[component_owners.ppb.'core:default']\nbase=0\nchannel=''\nlittle_endian=true\n",
        ).unwrap(),
        ..Project::default()
    };
    let mut app = App::new(project, false);
    app.snapshot.state = state::STOPPED.into();
    app.snapshot.register_session = 17;
    app.snapshot.generation = 3;
    app.select_pane(pane::REGS);
    app.view_rects[pane::REGS] = Rect::new(0, 0, 80, 20);
    app
}

fn physical(app: &App, id: &str, raw: &str) -> Sample {
    let mut value = sample(app, id, raw);
    value.view = SampleView::PhysicalCore;
    value.source = "mmio:ppb".into();
    let register = app
        .register_view
        .catalogue
        .as_ref()
        .unwrap()
        .register(id)
        .unwrap();
    let Reader::CorePrivate { address } = register.reader else {
        panic!("PPB definition required")
    };
    value.provenance = Some(serde_json::from_value(json!({
        "acquisition":"catalogue", "catalogue_reader":register.reader,
        "access":{"route":{"kind":"gdb_memory", "configured_endpoint":"localhost:3333", "endpoint":"localhost:3333",
            "address":format!("0x{address:x}"), "bits":32, "byte_order":"little"},
            "phase":"responded", "command":format!("-data-read-memory-bytes 0x{address:x} 4"),
            "context":value.context, "timestamp_ms":20, "completed_ms":23}
    })).unwrap());
    value
}

fn accept(app: &mut App, samples: Vec<Sample>) {
    let context = app.register_context();
    for value in &samples {
        app.register_view.values.insert(
            ("core:default".into(), value.id.clone(), "default".into()),
            value.clone(),
        );
        app.register_view.attempts.insert((
            context.session,
            context.generation,
            context.core.clone(),
            context.frame,
            value.id.clone(),
        ));
    }
    app.snapshot.register_samples.extend(samples);
    app.sync_register_capabilities();
}

fn cached<'a>(app: &'a App, id: &str) -> &'a Sample {
    &app.register_view.values[&("core:default".into(), id.into(), "default".into())]
}

#[test]
fn m4_identity_and_fpu_observations_preserve_valid_mpu_and_core_values() {
    let mut app = m4();
    let r0 = sample(&app, "r0", "0x20000da4");
    let cpuid = physical(&app, "scb.cpuid", "0x410fc241");
    accept(&mut app, vec![r0, cpuid]);
    let mpu: Vec<_> = [
        ("mpu.type", "0x00000800"),
        ("mpu.ctrl", "0"),
        ("mpu.rnr", "0"),
        ("mpu.rbar", "0"),
        ("mpu.rasr", "0"),
    ]
    .into_iter()
    .map(|(id, raw)| physical(&app, id, raw))
    .collect();
    accept(&mut app, mpu);
    let fpu: Vec<_> = [
        ("fpu.mvfr0", "0x10110021"),
        ("fpu.mvfr1", "0x11000011"),
        ("fpu.mvfr2", "0"),
        ("scb.cpacr", "0x00f00000"),
    ]
    .into_iter()
    .map(|(id, raw)| physical(&app, id, raw))
    .collect();
    accept(&mut app, fpu);
    for id in [
        "r0",
        "scb.cpuid",
        "mpu.type",
        "mpu.ctrl",
        "mpu.rnr",
        "mpu.rbar",
        "mpu.rasr",
        "fpu.mvfr0",
        "scb.cpacr",
    ] {
        assert_eq!(cached(&app, id).state, State::Valid, "{id}");
        let index = app
            .register_view
            .catalogue
            .as_ref()
            .unwrap()
            .registers
            .iter()
            .position(|r| r.id == id)
            .unwrap();
        assert_eq!(
            app.register_view
                .category(&app.project, &app.register_context(), index, true),
            status::Category::Valid,
            "{id}"
        );
    }
    // Reading newly eligible FP configuration fields must not gray earlier IDs.
    let config: Vec<_> = [
        ("fpu.fpccr", "0xc0000000"),
        ("fpu.fpcar", "0"),
        ("fpu.fpdscr", "0"),
    ]
    .into_iter()
    .map(|(id, raw)| physical(&app, id, raw))
    .collect();
    accept(&mut app, config);
    assert!(
        app.register_view
            .values
            .values()
            .all(|s| s.state == State::Valid)
    );
    app.sync_register_capabilities();
    let (engine, requests) = engine();
    app.register_view.rows = ["mpu.type", "mpu.ctrl", "fpu.mvfr0", "fpu.fpccr"]
        .into_iter()
        .map(|id| {
            Row::Register(
                app.register_view
                    .catalogue
                    .as_ref()
                    .unwrap()
                    .registers
                    .iter()
                    .position(|r| r.id == id)
                    .unwrap(),
                1,
            )
        })
        .collect();
    assert!(!app.ensure_registers(Some(&engine)));
    assert!(requests.try_recv().is_err());
}

#[test]
fn changed_enable_condition_retries_once_and_revokes_only_the_dependent_value() {
    use crate::registers::{
        Field, Segment,
        policy::{Compare, FieldCondition},
    };
    let mut app = super::tests::app();
    let catalogue = app.register_view.catalogue.as_mut().unwrap();
    let r0 = catalogue
        .registers
        .iter()
        .position(|r| r.id == "r0")
        .unwrap();
    let r1 = catalogue
        .registers
        .iter()
        .position(|r| r.id == "r1")
        .unwrap();
    catalogue.registers[r0].fields = vec![Field {
        name: "ENABLE".into(),
        segments: vec![Segment {
            offset: 0,
            width: 1,
        }],
        description: String::new(),
        access: None,
        enums: Vec::new(),
    }];
    catalogue.registers[r1].access_rule.need_enable = Some(FieldCondition {
        reg: "r0".into(),
        field: "ENABLE".into(),
        op: Compare::Eq,
        value: 1,
    });
    let observe = |app: &App, raw: &str| {
        let mut value = sample(app, "r0", raw);
        value.view = SampleView::PhysicalCore;
        value.provenance=Some(serde_json::from_value(json!({"acquisition":"catalogue","catalogue_reader":{"kind":"gdb","name":"r0"},
          "access":{"route":{"kind":"gdb_register","configured_endpoint":"localhost:3333","endpoint":"localhost:3333","name":"r0","index":0},
            "phase":"responded","command":"-data-list-register-values r 0","context":value.context,"timestamp_ms":20,"completed_ms":23}})).unwrap());
        value
    };
    let disabled = observe(&app, "0");
    accept(&mut app, vec![disabled]);
    let context = app.register_context();
    app.register_view.attempts.insert((
        context.session,
        context.generation,
        context.core.clone(),
        context.frame,
        "r1".into(),
    ));
    let enabled = observe(&app, "1");
    app.snapshot.register_samples.clear();
    accept(&mut app, vec![enabled]);
    app.register_view.rows = vec![Row::Register(r1, 1)];
    let (engine, requests) = engine();
    assert!(app.ensure_registers(Some(&engine)));
    let request = requests.try_recv().unwrap();
    assert_eq!(request.params["ids"], json!(["r1"]));
    let dependent = sample(&app, "r1", "0x1234");
    app.register_response(request.id, &json!({"samples":[dependent]}), None);
    app.sync_register_capabilities();
    app.register_view.rows = vec![Row::Register(r1, 1)];
    assert!(!app.ensure_registers(Some(&engine)));
    assert!(requests.try_recv().is_err());
    let mut peer = cached(&app, "r1").clone();
    peer.context.core = "peer".into();
    peer.owner = Some("core:peer".into());
    let peer_key = ("core:peer".into(), "r1".into(), "peer".into());
    app.register_view.values.insert(peer_key.clone(), peer);
    app.register_view.attempts.insert((
        context.session,
        context.generation,
        "peer".into(),
        context.frame,
        "r1".into(),
    ));
    let disabled = observe(&app, "0");
    app.snapshot.register_samples.clear();
    accept(&mut app, vec![disabled]);
    assert_eq!(cached(&app, "r0").state, State::Valid);
    assert_eq!(cached(&app, "r1").state, State::Stale);
    assert_eq!(app.register_view.values[&peer_key].state, State::Valid);
    assert!(app.register_view.attempts.contains(&(
        context.session,
        context.generation,
        "peer".into(),
        context.frame,
        "r1".into()
    )));
    app.register_view.rows = vec![Row::Register(r1, 1)];
    assert!(!app.ensure_registers(Some(&engine)));
    assert!(requests.try_recv().is_err());
    assert_eq!(
        cached(&app, "r1").value.as_ref().unwrap(),
        &RawValue::parse("0x1234", 32).unwrap()
    );
}
