use super::*;
use debugtui::registers::{Access, Catalogue, Condition, Reader};

fn run_probe(p: Project) -> (EngineHandle, Value) {
    p.validate().unwrap();
    let engine = coordinator::spawn(p);
    call(&engine, 1, "connect", json!({}));
    let list = call(&engine, 2, "registers_list", json!({}));
    let result = call(
        &engine,
        3,
        "registers_probe",
        json!({"context":list["context"]}),
    );
    (engine, result)
}

#[test]
fn probe_honors_selected_permissions_effects_conditions_and_reader_overrides() {
    for mode in [
        "write_only",
        "side_effect",
        "unknown",
        "excluded",
        "relocated",
    ] {
        let (mut p, out, mi, memory) = fixture("cortex-m4");
        let mut c = Catalogue::builtin("cortex-m4").unwrap();
        c.registers
            .retain(|r| matches!(r.id.as_str(), "scb.cpuid" | "fpu.mvfr2"));
        let r = c
            .registers
            .iter_mut()
            .find(|r| r.id == "fpu.mvfr2")
            .unwrap();
        match mode {
            "write_only" => r.access = Access::Wo,
            "side_effect" => r.read_side_effect = true,
            "unknown" | "excluded" => {
                r.conditions = vec![Condition {
                    fact: "fixture.optional".into(),
                    min: 1,
                    max: Some(1),
                }];
                if mode == "excluded" {
                    p.registers.facts.insert("fixture.optional".into(), 0);
                }
            }
            "relocated" => {
                r.reader = Reader::CorePrivate {
                    address: 0xe000ef4c,
                }
            }
            _ => unreachable!(),
        }
        p.registers.catalogue = out.join("selected.toml");
        fs::write(&p.registers.catalogue, toml::to_string(&c).unwrap()).unwrap();
        let mut bytes = values(0x410fc241, 2, 8, true);
        bytes["0xe000ef4c"] = json!("78563412");
        fs::write(
            memory,
            serde_json::to_vec(&json!({"localhost:4900":bytes})).unwrap(),
        )
        .unwrap();
        let (engine, result) = run_probe(p);
        let sample = result["probe"]["samples"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "fpu.mvfr2")
            .unwrap();
        let commands = reads(&mi).join("\n");
        assert!(!commands.contains("0xe000ef48"), "{mode}: {commands}");
        assert!(
            !commands.contains("0xe000e004"),
            "missing chip definitions must not fall back to builtin"
        );
        if mode == "relocated" {
            assert_eq!(sample["value"]["hex"], "0x12345678");
            assert!(commands.contains("0xe000ef4c"));
        } else {
            assert!(sample["provenance"]["access"].is_null(), "{mode}: {sample}");
            assert_ne!(sample["state"], "valid");
        }
        no_mutations(&mi);
        call(&engine, 90, "disconnect", json!({}));
        call(&engine, 91, "quit", json!({}));
    }
}

#[test]
fn f429_discovery_requires_rom_identity_power_and_current_enable_without_writes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let c = Catalogue::load(&root.join("profiles/registers/stm32f429.toml")).unwrap();
    for fault in [
        "none",
        "rom_absent",
        "wrong_pid",
        "powered_down",
        "trace_disabled",
    ] {
        let (mut p, _out, mi, memory) = fixture("cortex-m4");
        // Real chip projects use the coordinator even with one selected core.
        p.cores = vec![Core {
            name: "default".into(),
            endpoint: "localhost:4900".into(),
            ..Default::default()
        }];
        p.registers.catalogue = root.join("profiles/registers/stm32f429.toml");
        // Configured facts must not forge discovery observations.
        for r in &c.discovery.rules {
            p.registers.facts.insert(r.fact.clone(), 1);
        }
        let mut bytes = values(0x410fc241, 2, 8, fault != "trace_disabled");
        for r in &c.registers {
            if let Reader::CorePrivate { address } = r.reader {
                let key = format!("0x{address:x}");
                if bytes.get(&key).is_none() {
                    let v = (r.reset.unwrap_or(0) as u32).to_le_bytes();
                    bytes[&key] = json!(v.iter().map(|b| format!("{b:02x}")).collect::<String>());
                }
            }
        }
        bytes["0xe00ff010"] = json!(if fault == "rom_absent" {
            "0210f4ff"
        } else {
            "0310f4ff"
        });
        bytes["0xe00ff014"] = json!(if fault == "rom_absent" {
            "0220f4ff"
        } else {
            "0320f4ff"
        });
        bytes["0xe0040fc8"] = json!("a10c0000");
        if fault == "wrong_pid" {
            bytes["0xe0040fe0"] = json!("00000000");
            bytes["0xe0041fe0"] = json!("00000000");
        }
        if fault == "powered_down" {
            bytes["0xe0041314"] = json!("00000000");
        }
        fs::write(
            &memory,
            serde_json::to_vec(&json!({"localhost:4900":bytes})).unwrap(),
        )
        .unwrap();
        let (engine, result) = run_probe(p);
        let etm = &result["facts"]["stm32f429.etm_m4.present"];
        let tpiu = &result["facts"]["stm32f429.tpiu.m4_variant"];
        assert_eq!(etm == 1, fault == "none", "{fault}: {result}");
        assert_eq!(
            tpiu == 1,
            matches!(fault, "none" | "powered_down"),
            "{fault}: {result}"
        );
        assert_eq!(
            result["probe"]["facts"]["stm32f429.etm_m4.present"]["value"] == 1,
            fault == "none"
        );
        let snapshot = call(&engine, 31, "status", json!({}));
        assert_eq!(
            snapshot["register_probe"]["facts"]["stm32f429.etm_m4.present"]["value"] == 1,
            fault == "none"
        );
        let matrix = call(&engine, 32, "registers_matrix", json!({}));
        assert_eq!(
            matrix["probe"]["facts"]["stm32f429.etm_m4.present"]["value"] == 1,
            fault == "none"
        );
        assert!(!reads(&mi).join("\n").contains("0xe000ef48"));
        let read = call(
            &engine,
            4,
            "registers_read",
            json!({"ids":["etm.cr","etm.lar","tpiu.fifo0","tpiu.itatbctr0","nvic.iser3","systick.ctrl"]}),
        );
        assert_eq!(
            read["samples"][0]["state"] == "valid",
            fault == "none",
            "{fault}: {read}"
        );
        assert_eq!(read["samples"][1]["reason"], "reader_unsupported");
        for sample in &read["samples"].as_array().unwrap()[1..] {
            assert!(sample["provenance"]["access"].is_null());
        }
        if fault == "none" {
            // Same stop, later DEMCR=0 must revoke both discovery and old ITCTRL evidence.
            bytes["0xe000edfc"] = json!("00000000");
            fs::write(
                &memory,
                serde_json::to_vec(&json!({"localhost:4900":bytes})).unwrap(),
            )
            .unwrap();
            call(&engine, 5, "registers_read", json!({"ids":["dcb.demcr"]}));
            let list = call(&engine, 6, "registers_list", json!({}));
            assert!(list["facts"]["stm32f429.etm_m4.present"].is_null());
            assert!(list["facts"]["stm32f429.tpiu.m4_variant"].is_null());
            let before = reads(&mi).len();
            let guarded = call(
                &engine,
                7,
                "registers_read",
                json!({"ids":["tpiu.itatbctr0","etm.itmiscin"]}),
            );
            assert!(
                guarded["samples"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|s| s["provenance"]["access"].is_null())
            );
            assert_eq!(reads(&mi).len(), before);
        }
        no_mutations(&mi);
        call(&engine, 90, "disconnect", json!({}));
        call(&engine, 91, "quit", json!({}));
    }
}
