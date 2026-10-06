use super::*;
use crate::launch::tests::{Fixture, key};
use crate::registers::{RawValue, Sample, SampleView, State};
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn per_core_register_setup_preview_reports_effective_models_and_routes_without_probe() {
    let mut project = Project::default();
    project.registers.cpu = "cortex-r52".into();
    project.registers.tcl_endpoint = "localhost:6666".into();
    project.cores = vec![
        crate::config::Core {
            name: "m4".into(),
            endpoint: "localhost:3333".into(),
            registers: Some(
                toml::from_str(
                    "cpu='cortex-m4'\ntcl_endpoint='localhost:6667'\n[targets]\nm4='soc.m4'\n",
                )
                .unwrap(),
            ),
            ..Default::default()
        },
        crate::config::Core {
            name: "r52".into(),
            endpoint: "localhost:3334".into(),
            ..Default::default()
        },
    ];
    let loaded = project.registers.load().unwrap().unwrap();
    let lines = preview(&project, Some(&loaded), None, None).join("\n");
    assert!(lines.contains("Effective CPU [m4]: cortex-m4; Source: builtin:cortex-m4"));
    assert!(lines.contains("Effective CPU [r52]: cortex-r52; Source: builtin:cortex-r52"));
    assert!(lines.contains("Register route [m4]: endpoint localhost:6667; target soc.m4"));
    assert!(lines.contains("Register route [r52]: endpoint localhost:6666; target unspecified"));
    assert!(lines.contains("Observed CPU [m4]: Unknown"));
    assert!(lines.contains("No hardware access is performed"));
}

fn text(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        let mut x = 0;
        while x < buffer.area.width {
            let symbol = buffer[(x, y)].symbol();
            text.push_str(symbol);
            x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
        }
    }
    text
}
fn probe(context: &Context, midr: &str) -> Probe {
    let sample = Sample {
        id: "midr".into(),
        state: State::Valid,
        implementation: crate::registers::Implementation::Unknown,
        reason: crate::registers::Reason::Unknown,
        detail: String::new(),
        value: Some(RawValue::parse(midr, 32).unwrap()),
        owner: Some(format!("core:{}", context.core)),
        context: context.clone(),
        view: SampleView::PhysicalCore,
        owner_generation: None,
        provenance: None,
        last_value_provenance: None,
        eligibility: None,
        last_value_eligibility: None,
        timestamp_ms: 10,
        source: "identity-fixture".into(),
    };
    let mut probe = Probe {
        context: context.clone(),
        thread: "1".into(),
        identity: None,
        facts: Default::default(),
        samples: vec![sample],
        gdb_names: vec![],
        notes: vec![],
    };
    probe.decode();
    probe
}

#[test]
fn setup_cpu_catalogue_choices_preview_save_cancel_and_multicore_are_isolated() {
    // A child test process isolates config-directory changes from parallel tests
    // and never touches the customer's installed profiles.
    if env::var_os("DEBUGTUI_SETUP_REGISTER_TEST_CHILD").is_none() {
        let fixture = Fixture::new();
        let output = std::process::Command::new(env::current_exe().unwrap())
            .args(["--exact", "launch::registers::tests::setup_cpu_catalogue_choices_preview_save_cancel_and_multicore_are_isolated", "--nocapture"])
            .env("DEBUGTUI_CONFIG_DIR", &fixture.0)
            .env("DEBUGTUI_SETUP_REGISTER_TEST_CHILD", "1")
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let fixture = Fixture::new();
    let user = PathBuf::from(env::var_os("DEBUGTUI_CONFIG_DIR").unwrap()).join("profiles");
    fs::create_dir_all(user.join("registers")).unwrap();
    let devices =
        "version=1\n[devices.matrix]\ncores=[0,2]\nbackend='generic'\ncpu='cortex-r52+'\n";
    fs::write(user.join("devices.toml"), devices).unwrap();
    let profile = "backend='generic'\n[core_targets.\"0\"]\nendpoint='localhost:5000'\n[core_targets.\"2\"]\nendpoint='localhost:5002'\n[registers]\ncpu='cortex-r52'\ncatalogue='inherited.toml'\n";
    fs::write(fixture.0.join("tools/debug-env.toml"), profile).unwrap();
    let inherited = include_str!("../../../profiles/registers/cortex-m4.toml");
    fs::write(fixture.0.join("tools/inherited.toml"), inherited).unwrap();
    let project_text = "version=3\n[tools]\nprofile='tools/debug-env.toml'\n[debug]\nchip='matrix'\ncores=[0,2]\n[tasks]\nbuild='original-build'\ndownload='original-download'\n[registers]\nfacts={'customer.fact'=7}\n";
    let path = fixture.0.join("debug.toml");
    fs::write(&path, project_text).unwrap();
    let mut setup = Setup::new(Document::open(&path).unwrap());
    let unrelated = setup.document.raw.clone();
    for (index, cpu) in [(2, "cortex-m4"), (3, "cortex-r52"), (4, "cortex-r52+")] {
        setup.selected = CPU;
        let before = setup.document.raw.clone();
        setup.key(key(KeyCode::Enter));
        setup.picker.as_mut().unwrap().selected = index;
        let preview = setup.catalogue_preview().unwrap().join("\n");
        assert!(
            preview.contains(&format!("Source: builtin:{cpu}")),
            "{preview}"
        );
        assert!(preview.contains("Architecture:"));
        assert!(preview.contains("Chip association: matrix / cortex-r52+ (user)"));
        assert!(preview.contains("Observed CPU [core.0]: Unknown"));
        assert!(preview.contains("Observed CPU [core.2]: Unknown"));
        assert_eq!(
            preview.contains("Warning: chip association"),
            cpu != "cortex-r52+"
        );
        setup.key(key(KeyCode::F(1)));
        assert!(setup.register_details.is_some());
        setup.key(key(KeyCode::F(5)));
        setup.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(!setup.pending);
        assert_eq!(setup.document.raw, before);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            if index == 2 {
                project_text.to_owned()
            } else {
                toml::to_string_pretty(&before).unwrap()
            }
        );
        setup.key(key(KeyCode::Esc));
        assert!(setup.picker.is_some());
        setup.key(key(KeyCode::Esc));
        assert_eq!(setup.document.raw, before);
        setup.key(key(KeyCode::Enter));
        setup.picker.as_mut().unwrap().selected = index;
        setup.key(key(KeyCode::Enter));
        assert!(setup.picker.is_none(), "{}", setup.message);
        assert_eq!(setup.document.raw["registers"]["cpu"].as_str(), Some(cpu));
        assert_eq!(
            setup.document.raw["registers"]["catalogue"].as_str(),
            Some("")
        );
        let project = setup.document.project().unwrap();
        assert_eq!(project.tasks.build, "original-build");
        assert_eq!(project.tasks.download, "original-download");
        assert_eq!(
            project
                .cores
                .iter()
                .map(|core| (core.name.as_str(), core.endpoint.as_str()))
                .collect::<Vec<_>>(),
            [("core.0", "localhost:5000"), ("core.2", "localhost:5002")]
        );
        assert_eq!(project.registers.facts["customer.fact"], 7);
        let mut other = setup.document.raw.clone();
        other.as_table_mut().unwrap().remove("registers");
        let mut expected = unrelated.clone();
        expected.as_table_mut().unwrap().remove("registers");
        assert_eq!(other, expected);
        setup.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        let reloaded = Document::open(&path).unwrap().project().unwrap();
        assert_eq!(reloaded.registers.load().unwrap().unwrap().0.cpu, cpu);
    }
    let customer = user.join("registers").join("customer.toml");
    fs::write(
        &customer,
        include_str!("../../../profiles/registers/cortex-r52+.toml"),
    )
    .unwrap();
    setup.key(key(KeyCode::Enter));
    let picker = setup.picker.as_mut().unwrap();
    picker.selected = picker
        .choices
        .iter()
        .position(|choice| choice.label() == "customer / user")
        .unwrap();
    let customer_preview = setup.catalogue_preview().unwrap().join("\n");
    assert!(
        customer_preview.contains(&format!("Source: user:{}", customer.display())),
        "{customer_preview}\nExpected user path: {}",
        customer.display()
    );
    assert!(customer_preview.contains("A user preset name can differ from its declared CPU"));
    assert!(!customer_preview.contains("The catalogue file takes precedence"));
    setup.key(key(KeyCode::Enter));
    assert!(
        setup
            .help_text()
            .contains("Warning: configured CPU customer")
    );
    setup.save_document().unwrap();
    assert_eq!(
        Document::open(&path)
            .unwrap()
            .project()
            .unwrap()
            .registers
            .cpu,
        "customer"
    );

    // Existing invalid overrides stay errors and keep the draft/picker intact.
    let override_path = user.join("registers/cortex-r52+.toml");
    fs::write(&override_path, "broken=[").unwrap();
    let before = setup.document.raw.clone();
    setup.key(key(KeyCode::Enter));
    setup.picker.as_mut().unwrap().selected = 4;
    assert!(setup.catalogue_preview().is_err());
    setup.key(key(KeyCode::Enter));
    assert!(setup.message.starts_with("Error:"));
    assert!(setup.picker.is_some());
    assert_eq!(setup.document.raw, before);
    fs::remove_file(&override_path).unwrap();
    setup.picker.as_mut().unwrap().selected = 1;
    setup.key(key(KeyCode::Enter));
    assert!(
        setup
            .document
            .project()
            .unwrap()
            .registers
            .load()
            .unwrap()
            .is_none()
    );
    setup.key(key(KeyCode::Enter));
    setup.picker.as_mut().unwrap().selected = 0;
    setup.key(key(KeyCode::Enter));
    let loaded = setup
        .document
        .project()
        .unwrap()
        .registers
        .load()
        .unwrap()
        .unwrap();
    assert_eq!(loaded.0.cpu, "cortex-m4");
    assert!(loaded.1.starts_with("file:"));
    assert!(
        setup
            .help_text()
            .contains("Warning: configured CPU cortex-r52")
    );
    assert!(setup.document.raw["registers"].get("cpu").is_none());
    assert!(setup.document.raw["registers"].get("catalogue").is_none());

    // CPU picker's F2 browses catalogues, rather than replacing the project.
    let catalogue = fixture.0.join("客户 寄存器.toml");
    fs::write(
        &catalogue,
        include_str!("../../../profiles/registers/cortex-r52+.toml"),
    )
    .unwrap();
    setup.key(key(KeyCode::Enter));
    setup.key(key(KeyCode::F(2)));
    assert_eq!(setup.selected, CATALOGUE);
    assert!(setup.browser.as_ref().unwrap().toml_only);
    setup.browser = Some(Browser::open(&fixture.0, true).unwrap());
    let browser = setup.browser.as_mut().unwrap();
    browser.selected = browser
        .entries
        .iter()
        .position(|entry| entry == &fs::canonicalize(&catalogue).unwrap())
        .unwrap();
    let before = setup.document.raw.clone();
    assert!(
        setup
            .catalogue_preview()
            .unwrap()
            .join("\n")
            .contains("CPU: cortex-r52+")
    );
    setup.key(key(KeyCode::F(1)));
    setup.key(key(KeyCode::Esc));
    setup.key(key(KeyCode::Esc));
    assert_eq!(setup.document.raw, before);
    let broken = fixture.0.join("broken.toml");
    fs::write(&broken, "version=999").unwrap();
    setup.open_browser().unwrap();
    assert!(setup.choose_path(&broken).is_err());
    assert!(setup.browser.is_some());
    assert_eq!(setup.document.raw, before);
    setup.choose_path(&catalogue).unwrap();
    assert_eq!(
        setup.document.raw["registers"]["catalogue"].as_str(),
        Some("客户 寄存器.toml")
    );
    // A file remains the actual source even with an explicit empty CPU preset.
    setup.document.select_register_cpu(Some(""));
    setup.choose_path(&catalogue).unwrap();
    assert_eq!(setup.values()[CPU], "Catalogue file (no CPU preset)");
    setup.save_document().unwrap();
    assert_eq!(
        Document::open(&path)
            .unwrap()
            .project()
            .unwrap()
            .registers
            .load()
            .unwrap()
            .unwrap()
            .0
            .cpu,
        "cortex-r52+"
    );
    let saved = fs::read(&path).unwrap();
    setup.key(key(KeyCode::Enter));
    setup.editor.as_mut().unwrap().text = "missing catalogue.toml".into();
    setup.key(key(KeyCode::F(1)));
    assert!(setup.register_details.is_some());
    assert!(setup.catalogue_preview().is_err());
    setup.key(key(KeyCode::Esc));
    setup.key(key(KeyCode::Enter));
    assert!(setup.message.starts_with("Error:"));
    assert!(setup.editor.is_some());
    setup.key(key(KeyCode::Esc));
    assert_eq!(fs::read(&path).unwrap(), saved);
    assert_eq!(
        fs::read_to_string(user.join("devices.toml")).unwrap(),
        devices
    );
    assert_eq!(
        fs::read_to_string(fixture.0.join("tools/debug-env.toml")).unwrap(),
        profile
    );
    assert_eq!(
        fs::read_to_string(fixture.0.join("tools/inherited.toml")).unwrap(),
        inherited
    );
    assert_eq!(
        fs::read_to_string(&customer).unwrap(),
        include_str!("../../../profiles/registers/cortex-r52+.toml")
    );
}

#[test]
fn setup_catalogue_observed_identity_is_current_target_and_core_specific() {
    let project = Project {
        cores: vec![
            crate::config::Core {
                name: "core.0".into(),
                endpoint: "localhost:5000".into(),
                ..Default::default()
            },
            crate::config::Core {
                name: "core.2".into(),
                endpoint: "localhost:5002".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let context = Context {
        session: 9,
        generation: 3,
        core: "core.0".into(),
        frame: 0,
    };
    let evidence = probe(&context, "0x411fd134");
    let loaded = (
        Catalogue::builtin("cortex-m4").unwrap(),
        "builtin:cortex-m4".into(),
    );
    let observation = Observation::current(&project, Some(&evidence), &context, true).unwrap();
    let lines = preview(&project, Some(&loaded), None, Some(&observation)).join("\n");
    assert!(lines.contains("Observed CPU [core.0]: Cortex-R52"));
    assert!(lines.contains("Warning: observed CPU [core.0]"));
    assert!(lines.contains("Observed CPU [core.2]: Unknown"));
    assert!(!lines.contains("Warning: observed CPU [core.2]"));
    for change in ["run", "session", "stop", "core", "frame"] {
        let mut current = context.clone();
        match change {
            "session" => current.session += 1,
            "stop" => current.generation += 1,
            "core" => current.core = "core.2".into(),
            "frame" => current.frame += 1,
            _ => {}
        }
        assert!(
            Observation::current(&project, Some(&evidence), &current, change != "run").is_none(),
            "{change}"
        );
    }
    for change in [
        "endpoint",
        "chip",
        "core",
        "mode",
        "gdb",
        "init",
        "service",
        "tcl",
        "target",
        "command",
        "live-watch",
    ] {
        let mut draft = project.clone();
        match change {
            "endpoint" => draft.cores[0].endpoint = "localhost:7777".into(),
            "chip" => draft.debug.chip = "other".into(),
            "core" => draft.cores[0].name = "other".into(),
            "mode" => draft.target.mode = "local".into(),
            "gdb" => draft.gdb.executable = "other-gdb".into(),
            "init" => draft.cores[0].init.push("reset".into()),
            "service" => draft.service = Some(crate::config::Service::default()),
            "tcl" => draft.registers.tcl_endpoint = "localhost:7777".into(),
            "target" => {
                draft
                    .registers
                    .targets
                    .insert("core.0".into(), "physical.core2".into());
            }
            "command" => draft.registers.cp15_command = "arm mrc".into(),
            "live-watch" => draft.live_watch = Some(crate::config::LiveWatchConfig::default()),
            _ => unreachable!(),
        }
        let lines = preview(&draft, Some(&loaded), None, Some(&observation)).join("\n");
        assert!(!lines.contains("Cortex-R52"), "{change}: {lines}");
        assert!(!lines.contains("Warning: observed CPU"));
    }
    let mut draft = project.clone();
    draft.registers.cpu = "cortex-r52+".into();
    draft.registers.catalogue = "a-different-catalogue.toml".into();
    assert!(
        preview(&draft, Some(&loaded), None, Some(&observation))
            .join("\n")
            .contains("Observed CPU [core.0]: Cortex-R52")
    );
    let matching = (
        Catalogue::builtin("cortex-r52").unwrap(),
        "builtin:cortex-r52".into(),
    );
    assert!(
        !preview(&project, Some(&matching), None, Some(&observation))
            .join("\n")
            .contains("Warning: observed CPU")
    );
    let unadapted = probe(&context, "0x411fd144");
    assert!(unadapted.identity.as_ref().unwrap().model.is_none());
    let unknown = Observation::current(&project, Some(&unadapted), &context, true).unwrap();
    assert!(
        !preview(&project, Some(&loaded), None, Some(&unknown))
            .join("\n")
            .contains("Warning: observed CPU")
    );
    assert!(mismatch("observed CPU", "Cortex-R52", "cortex-r52").is_none());
    assert!(mismatch("observed CPU", "Cortex-R52", "cortex-r52+").is_some());
}

#[test]
fn setup_catalogue_chip_association_preview_matches_default_resolution_without_identity_claims() {
    let mut catalogue = crate::devices::Catalogue::parse(crate::devices::DEFAULTS).unwrap();
    let mut project = Project::default();
    project.debug.chip = "tha6206".into();
    let loaded = (
        Catalogue::builtin("cortex-m4").unwrap(),
        "builtin:cortex-m4".into(),
    );
    for (cpu, source) in [
        ("cortex-r52", "user"),
        ("cortex-r52+", "user"),
        ("", "builtin"),
    ] {
        catalogue.devices.get_mut("tha6206").unwrap().cpu = cpu.into();
        let association = catalogue.cpu_association("tha6206").unwrap().unwrap();
        assert_eq!(association.1, source);
        let lines = preview(&project, Some(&loaded), Some(&association), None).join("\n");
        assert!(lines.contains("Warning: chip association"));
        assert!(lines.contains("configuration only"));
        assert!(lines.contains("Observed CPU [default]: Unknown"));
    }
    assert!(catalogue.cpu_association("unknown").unwrap().is_none());
    assert!(catalogue.cpu_association("tha6104").unwrap().is_none());
}

#[test]
fn setup_catalogue_details_scroll_all_conditions_and_unicode_at_supported_sizes_without_mutation() {
    let fixture = Fixture::new();
    let mut catalogue = Catalogue::builtin("cortex-r52+").unwrap();
    catalogue.description = "客户寄存器说明：仅定义不证明当前硬件能力。".repeat(5);
    catalogue.registers[0]
        .conditions
        .push(crate::registers::Condition {
            fact: "customer.range".into(),
            min: 2,
            max: Some(3),
        });
    let file = fixture.0.join("目录 空格.toml");
    fs::write(&file, toml::to_string(&catalogue).unwrap()).unwrap();
    let mut document = Document::empty(fixture.0.join("debug.toml"));
    document.set_path("registers", "catalogue", &file);
    let before = document.raw.clone();
    for (width, height) in [(45, 12), (80, 24), (120, 36)] {
        let mut setup = Setup::new(document.clone());
        setup.selected = CATALOGUE;
        setup.key(key(KeyCode::F(1)));
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut seen = String::new();
        loop {
            terminal.draw(|f| setup.draw(f)).unwrap();
            seen.push_str(&text(&terminal));
            let details = setup.register_details.as_ref().unwrap();
            if details.scroll == details.max_scroll {
                break;
            }
            setup.key(key(KeyCode::Down));
        }
        let compact: String = seen
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '│')
            .collect();
        for expected in [
            "CPU:cortex-r52+",
            "Architecture:armv8-r-aarch32",
            "客户寄存器说明",
            "Supportconditions",
            "Factcustomer.range:>=2and<=3",
            "UnknownisnotNotimplemented",
            "Nohardwareaccessisperformed",
            "Esc/Closereturnstothedraft",
        ] {
            assert!(
                compact.contains(expected),
                "{width}x{height}: missing {expected}"
            );
        }
        setup.key(key(KeyCode::Home));
        terminal.draw(|f| setup.draw(f)).unwrap();
        assert_eq!(setup.register_details.as_ref().unwrap().scroll, 0);
        setup.mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 2,
            row: 2,
            modifiers: KeyModifiers::NONE,
        });
        assert!(setup.register_details.as_ref().unwrap().scroll > 0);
        let close = setup.register_details.as_ref().unwrap().close;
        setup.mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: close.x,
            row: close.y,
            modifiers: KeyModifiers::NONE,
        });
        assert!(setup.register_details.is_none());
        assert_eq!(setup.document.raw, before);
        assert!(!setup.pending);
        assert!(!setup.document.path.exists());
    }
}

#[test]
fn setup_catalogue_modal_keeps_its_opening_snapshot_and_reopening_reloads_parse_errors() {
    let fixture = Fixture::new();
    let mut catalogue = Catalogue::builtin("cortex-m4").unwrap();
    catalogue.description = "FIRST_CATALOGUE_SNAPSHOT".into();
    let file = fixture.0.join("customer.toml");
    fs::write(&file, toml::to_string(&catalogue).unwrap()).unwrap();
    let mut document = Document::empty(fixture.0.join("debug.toml"));
    document.set_path("registers", "catalogue", &file);
    let before = document.raw.clone();
    let mut setup = Setup::new(document);
    setup.selected = CATALOGUE;
    setup.key(key(KeyCode::F(1)));
    fs::write(&file, "invalid catalogue = [").unwrap();
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal.draw(|f| setup.draw(f)).unwrap();
    assert!(text(&terminal).contains("FIRST_CATALOGUE_SNAPSHOT"));
    // Only the visible modal is a snapshot. Explicit loading still sees errors.
    assert!(setup.catalogue_preview().is_err());
    setup.key(key(KeyCode::Esc));
    setup.key(key(KeyCode::F(1)));
    terminal.draw(|f| setup.draw(f)).unwrap();
    assert!(text(&terminal).contains("Catalogue error:"));
    assert!(!text(&terminal).contains("FIRST_CATALOGUE_SNAPSHOT"));
    assert_eq!(setup.document.raw, before);
}
