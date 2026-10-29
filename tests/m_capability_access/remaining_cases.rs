use super::*;
use debugtui::registers::{Catalogue, Reader};

fn word(bytes: &mut Value, address: u32, value: u32) {
    bytes[format!("0x{address:x}")] = json!(
        value
            .to_le_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
}

#[test]
fn controlled_optional_reads_require_current_identity_rom_and_enable_without_presence_claims() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for fault in [
        "none",
        "zero",
        "pc",
        "lsr_nonzero",
        "trace_off",
        "rom_absent",
        "format",
        "offset",
        "wrong_cpu",
    ] {
        let (mut p, out, mi, memory) = fixture("cortex-m4");
        let mut c = Catalogue::load(&root.join("profiles/registers/stm32f429.toml")).unwrap();
        // Alias declares no restriction of its own: the parent's access restriction must propagate.
        let mut alias = c.register("etm.lar").unwrap().clone();
        alias.id = "fixture.lock_alias".into();
        alias.reader = Reader::Alias {
            source: "etm.lar".into(),
            offset: 0,
        };
        alias.access_rule = Default::default();
        alias.conditions.clear();
        c.registers.push(alias);
        c.discovery.registers.push("fixture.lock_alias".into());
        p.registers.catalogue = out.join("selected.toml");
        fs::write(&p.registers.catalogue, toml::to_string(&c).unwrap()).unwrap();
        // These historical declarations must neither grant access nor establish a function.
        for fact in ["stm32f429.dwt.pcsr.present", "stm32f429.itm.software_lock"] {
            p.registers.facts.insert(fact.into(), 1);
        }
        let mut bytes = values(
            if fault == "wrong_cpu" {
                0x410fc231
            } else {
                0x410fc241
            },
            2,
            8,
            fault != "trace_off",
        );
        for r in &c.registers {
            if let Reader::CorePrivate { address } = r.reader
                && bytes.get(format!("0x{address:x}")).is_none()
            {
                word(&mut bytes, address as u32, r.reset.unwrap_or(0) as u32);
            }
        }
        let rom = |value: u32| match fault {
            "rom_absent" => value & !1,
            "format" => value & !2,
            "offset" => value ^ 0x1000,
            _ => value,
        };
        word(&mut bytes, 0xe00ff004, rom(0xfff02003));
        word(&mut bytes, 0xe00ff00c, rom(0xfff01003));
        let pc = match fault {
            "zero" => 0,
            "pc" => 0x08001234,
            _ => 0xffffffff,
        };
        word(&mut bytes, 0xe000101c, pc);
        let lsr = u32::from(fault == "lsr_nonzero");
        word(&mut bytes, 0xe0000fb4, lsr);
        fs::write(
            &memory,
            serde_json::to_vec(&json!({"localhost:4900":bytes})).unwrap(),
        )
        .unwrap();
        let (engine, probe) = super::discovery_cases::run_probe(p);
        let allowed = matches!(fault, "none" | "zero" | "pc" | "lsr_nonzero");
        for id in ["dwt.pcsr", "itm.lsr"] {
            let s = probe["probe"]["samples"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["id"] == id)
                .unwrap();
            assert_eq!(s["state"] == "valid", allowed, "{fault}/{id}: {s}");
            assert_eq!(
                s["implementation"], "unknown",
                "read permission is not function implementation"
            );
            assert_eq!(s["provenance"]["access"].is_null(), !allowed);
        }
        assert!(probe["probe"]["facts"]["stm32f429.dwt.pcsr.present"].is_null());
        assert!(probe["probe"]["facts"]["stm32f429.itm.software_lock"].is_null());
        for id in ["etm.lar", "fixture.lock_alias"] {
            let s = probe["probe"]["samples"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["id"] == id)
                .unwrap();
            assert_eq!(s["reason"], "access_restricted", "{fault}/{id}: {s}");
            assert!(s["provenance"]["access"].is_null());
        }
        for manual in [false, true] {
            let result = call(
                &engine,
                10 + u64::from(manual),
                "registers_read",
                json!({"ids":["etm.lar", "fixture.lock_alias", "dwt.pcsr", "itm.lsr"],"manual":manual}),
            );
            for s in &result["samples"].as_array().unwrap()[..2] {
                assert_eq!(s["state"], "unavailable");
                assert_eq!(s["reason"], "access_restricted");
                assert!(s["detail"].as_str().unwrap().contains("UNPREDICTABLE"));
                assert!(s["provenance"]["access"].is_null());
            }
            assert_eq!(result["samples"][2]["state"] == "valid", allowed);
            assert_eq!(result["samples"][3]["state"] == "valid", allowed);
            if allowed {
                assert_eq!(result["samples"][2]["value"]["hex"], format!("0x{pc:08x}"));
                assert_eq!(result["samples"][3]["value"]["hex"], format!("0x{lsr:08x}"));
                assert_eq!(result["samples"][2]["implementation"], "unknown");
                assert_eq!(result["samples"][3]["implementation"], "unknown");
                assert_eq!(result["statistics"]["causes"]["access_restricted"], 2);
                assert_eq!(result["statistics"]["causes"]["valid"], 2);
            }
            assert!(result["statistics"]["causes"]["reader_unsupported"].is_null());
        }
        let protected = call(
            &engine,
            20,
            "registers_read",
            json!({"ids":["fpu.mvfr2","scs.pid5","scs.pid6","scs.pid7"]}),
        );
        assert_eq!(protected["statistics"]["no_io"], 4);
        for s in protected["samples"].as_array().unwrap() {
            assert_eq!(s["implementation"], "unknown");
            assert_ne!(s["state"], "valid");
        }
        let matrix = call(&engine, 21, "registers_matrix", json!({}));
        for id in ["etm.lar", "fixture.lock_alias"] {
            let row = matrix["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == id)
                .unwrap();
            assert_eq!(row["support"], "access_restricted");
            assert_eq!(row["automatic_eligible"], false);
            assert_eq!(row["plan"]["available"], false);
        }
        if allowed {
            word(&mut bytes, 0xe000edfc, 0);
            fs::write(
                &memory,
                serde_json::to_vec(&json!({"localhost:4900":bytes})).unwrap(),
            )
            .unwrap();
            call(&engine, 22, "registers_read", json!({"ids":["dcb.demcr"]}));
            let denied = call(
                &engine,
                23,
                "registers_read",
                json!({"ids":["dwt.pcsr","itm.lsr"],"manual":true}),
            );
            assert_eq!(denied["statistics"]["no_io"], 2);
            assert_eq!(denied["statistics"]["causes"]["feature_disabled"], 2);
        }
        let commands = reads(&mi).join("\n");
        for address in [
            "0xe0041fb0",
            "0xe000ef48",
            "0xe000efd4",
            "0xe000efd8",
            "0xe000efdc",
        ] {
            assert!(
                !commands.contains(address),
                "{fault}: unauthorized read {address}"
            );
        }
        if !allowed {
            assert!(!commands.contains("0xe000101c"));
            assert!(!commands.contains("0xe0000fb4"));
        }
        no_mutations(&mi);
        call(&engine, 90, "disconnect", json!({}));
        call(&engine, 91, "quit", json!({}));
    }
}
