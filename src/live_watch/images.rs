//! Resident-image raw monitoring. The binding is explicit or last-known from a
//! stop; polling does not infer an execution trace or halt a running processor.
use super::*;
use crate::{
    config::Project,
    images::{Image, Loaded},
    session::Snapshot,
};

struct Binding {
    name: String,
    image: Image,
    loaded: Loaded,
    symbol: Symbol,
    endpoint: String,
    target: String,
    stale: Option<String>,
}

fn bindings(project: &Project, snapshot: &Snapshot) -> Result<Vec<Binding>, String> {
    let state = snapshot.image_state.as_ref().ok_or("No resident images")?;
    if !state.symbols_loaded || !state.error.is_empty() {
        return Err("Resident image symbols are unavailable or stale".into());
    }
    let core = project.preference_core.as_deref().unwrap_or("default");
    let mut result = Vec::new();
    for watch in &snapshot.watches {
        let (explicit, expression) = crate::images::scoped_expression(&watch.name)?;
        if expression.is_empty()
            || !expression
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || expression.as_bytes()[0].is_ascii_digit()
        {
            continue;
        }
        if explicit.is_none() && snapshot.locals.iter().any(|v| v.name == expression) {
            continue;
        }
        let id = explicit
            .or_else(|| watch.image_origin.as_ref().map(|o| o.image.as_str()))
            .or(state.selected_frame.image.as_deref())
            .ok_or("Live global has no verified last-known image; use @image:global")?;
        let loaded = state
            .images
            .iter()
            .find(|i| i.id == id)
            .ok_or("Live image is not configured for this core")?
            .clone();
        let image = project
            .images
            .iter()
            .find(|i| i.id == id)
            .cloned()
            .unwrap_or_else(|| Image {
                id: id.into(),
                elf: loaded.elf.clone(),
                ..Default::default()
            });
        let Some(symbol) = image_objects(&image, &loaded)?
            .get(expression)
            .copied()
            .filter(|s| matches!(s.size, 1 | 2 | 4))
        else {
            continue;
        };
        let channel = if image.memory_channel.is_empty() {
            image
                .saved_context
                .as_ref()
                .map(|c| c.channel.as_str())
                .unwrap_or_default()
        } else {
            &image.memory_channel
        };
        let access = project.memory_access.iter().find(|c| c.id == channel && c.while_running && (c.cores.is_empty() || c.cores.iter().any(|c| c == core))).ok_or("Image live Watch needs an explicit channel supporting reads while running on this core")?;
        result.push(Binding {
            name: watch.name.clone(),
            image,
            loaded,
            symbol,
            endpoint: access.tcl_endpoint.clone(),
            target: access.target.clone(),
            stale: None,
        });
    }
    Ok(result)
}

pub(crate) fn spawn_images(
    project: &Project,
    snapshot: &Snapshot,
    notify: Doorbell,
) -> Result<LiveWatchHandle, String> {
    let mut bindings = bindings(project, snapshot)?;
    let interval = project
        .live_watch
        .as_ref()
        .ok_or("Live Watch is not enabled")?
        .interval_ms
        .clamp(10, 60_000);
    let (tx, rx) = mpsc::sync_channel(64);
    let cancellation = Arc::new(AtomicBool::new(false));
    let cancel = cancellation.clone();
    thread::spawn(move || {
        let mut connection: Option<(String, TcpStream)> = None;
        while !cancel.load(Ordering::Relaxed) && !bindings.is_empty() {
            for binding in &mut bindings {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let result = (|| {
                    if binding.stale.is_none() {
                        binding.stale = match crate::images::changed(&binding.loaded, &binding.image) {
                            Ok(false)=>None, Ok(true)=>Some("ELF content changed; live binding expired until symbols are explicitly reloaded".into()), Err(e)=>Some(e),
                        };
                    }
                    if let Some(error) = &binding.stale {
                        return Err(error.clone());
                    }
                    if connection
                        .as_ref()
                        .is_none_or(|(e, _)| e != &binding.endpoint)
                    {
                        connection = Some((binding.endpoint.clone(), connect(&binding.endpoint)?));
                    }
                    let text = read_only_transaction(
                        &mut connection.as_mut().unwrap().1,
                        &format!(
                            "{} read_memory 0x{:x} {} 1",
                            word(&binding.target),
                            binding.symbol.address,
                            binding.symbol.size * 8
                        ),
                    )?;
                    parse_value(&text).ok_or_else(|| format!("Invalid live memory value: {text}"))
                })();
                if result.is_err() && binding.stale.is_none() {
                    connection = None;
                }
                let (value, error) = match result {
                    Ok(value) => (Some(value), None),
                    Err(error) => (None, Some(error)),
                };
                let _ = tx.try_send(Event::LiveWatch {
                    sample: LiveWatchSample {
                        expression: binding.name.clone(),
                        address: binding.symbol.address,
                        bits: (binding.symbol.size * 8) as u32,
                        value,
                        error,
                        timestamp: Stamp::now().wall,
                        core: None,
                        generation: 0,
                        image: Some(binding.image.id.clone()),
                        image_revision: binding.loaded.content_revision.clone(),
                    },
                });
                notify.ring();
            }
            wait(&cancel, interval);
        }
    });
    Ok(LiveWatchHandle {
        events: rx,
        cancellation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::MemoryAccess, images, session::Variable};
    fn elf() -> Vec<u8> {
        let mut b = vec![0; 585];
        b[..7].copy_from_slice(b"\x7fELF\x01\x01\x01");
        for (at, n) in [(16, 2u16), (18, 40), (40, 52), (46, 40), (48, 7), (50, 4)] {
            b[at..at + 2].copy_from_slice(&n.to_le_bytes());
        }
        b[32..36].copy_from_slice(&256u32.to_le_bytes());
        let names = b"\0.text\0.data\0.debug_info\0.shstrtab\0.symtab\0.strtab\0";
        b[128..128 + names.len()].copy_from_slice(names);
        for (i, name, kind, flags, addr, offset, size, link, stride) in [
            (1, 1, 1, 6, 0x1000, 64, 4, 0, 0),
            (2, 7, 1, 3, 0x2000, 68, 4, 0, 0),
            (3, 13, 1, 0, 0, 72, 1, 0, 0),
            (4, 25, 3, 0, 0, 128, names.len() as u32, 0, 0),
            (5, 35, 2, 0, 0, 544, 32, 6, 16),
            (6, 43, 3, 0, 0, 576, 9, 0, 0),
        ] {
            for (field, n) in [
                (0, name),
                (4, kind),
                (8, flags),
                (12, addr),
                (16, offset),
                (20, size),
                (24, link),
                (36, stride),
            ] {
                let at = 256 + i * 40 + field;
                b[at..at + 4].copy_from_slice(&n.to_le_bytes());
            }
        }
        for (at, n) in [(560, 1u32), (564, 0x2000), (568, 4)] {
            b[at..at + 4].copy_from_slice(&n.to_le_bytes());
        }
        b[572] = 0x11;
        b[574..576].copy_from_slice(&2u16.to_le_bytes());
        b[576..].copy_from_slice(b"\0counter\0");
        b
    }
    fn fixture(file: &Path, endpoint: String) -> (Project, Snapshot) {
        let image = Image {
            id: "vm0".into(),
            elf: file.into(),
            offset: 0x10000,
            memory_channel: "ahb".into(),
            cores: vec!["core.0".into()],
            ..Default::default()
        };
        let loaded = images::inspect(&image, false).unwrap();
        let mut p = Project {
            images: vec![image],
            preference_core: Some("core.0".into()),
            live_watch: Some(LiveWatchConfig {
                interval_ms: 20,
                ..Default::default()
            }),
            ..Default::default()
        };
        p.memory_access.push(MemoryAccess {
            id: "ahb".into(),
            target: "test.AHB".into(),
            tcl_endpoint: endpoint,
            while_running: true,
            cores: vec!["core.0".into()],
            ..Default::default()
        });
        let s = Snapshot {
            image_state: Some(images::State {
                symbols_loaded: true,
                images: vec![loaded],
                selected_frame: images::Resolution {
                    image: Some("vm0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }),
            watches: vec![Variable {
                name: "@vm0:counter".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        (p, s)
    }
    #[test]
    fn image_live_binding_keeps_relocation_core_and_content_revision_and_stops_stale_reads() {
        let file =
            std::env::temp_dir().join(format!("debugtui-image-live-{}.elf", std::process::id()));
        fs::write(&file, elf()).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let (p, mut snapshot) = fixture(&file, listener.local_addr().unwrap().to_string());
        let b = bindings(&p, &snapshot).unwrap();
        assert_eq!(b[0].symbol.address, 0x12000);
        snapshot.watches[0].name = "@vm1:counter".into();
        assert!(bindings(&p, &snapshot).is_err());
        snapshot.watches[0].name = "counter".into();
        snapshot.locals.push(Variable {
            name: "counter".into(),
            ..Default::default()
        });
        assert!(bindings(&p, &snapshot).unwrap().is_empty());
        snapshot.locals.clear();
        snapshot.watches[0].name = "@vm0:counter".into();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut commands = Vec::new();
            loop {
                let mut command = Vec::new();
                let mut byte = [0];
                while stream.read_exact(&mut byte).is_ok() && byte[0] != 0x1a {
                    command.push(byte[0]);
                }
                if command.is_empty() {
                    break;
                }
                let command = String::from_utf8(command).unwrap();
                assert!(command.contains("read_memory 0x12000 32 1"));
                assert!(!command.contains("halt"));
                commands.push(command);
                if stream.write_all(b"0:123\x1a").is_err() {
                    break;
                }
            }
            commands
        });
        let handle = spawn_images(&p, &snapshot, Doorbell::default()).unwrap();
        let Event::LiveWatch { sample } =
            handle.events.recv_timeout(Duration::from_secs(3)).unwrap()
        else {
            panic!()
        };
        assert_eq!(sample.value, Some(123));
        assert_eq!(sample.image.as_deref(), Some("vm0"));
        assert_eq!(
            sample.image_revision,
            snapshot.image_state.as_ref().unwrap().images[0].content_revision
        );
        let mut changed = elf();
        changed[68] = 1;
        fs::write(&file, changed).unwrap();
        loop {
            let Event::LiveWatch { sample } =
                handle.events.recv_timeout(Duration::from_secs(3)).unwrap()
            else {
                panic!()
            };
            if let Some(error) = sample.error {
                assert!(error.contains("ELF content changed"));
                assert_eq!(sample.value, None);
                break;
            }
        }
        drop(handle);
        assert!(!server.join().unwrap().is_empty());
        fs::remove_file(file).unwrap();
    }
}
