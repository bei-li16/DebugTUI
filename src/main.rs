use debugtui::{
    config::Project,
    session::{self, Event, Request},
    ui,
};
use serde_json::json;
use std::{
    env, fs,
    io::{self, BufRead, Write},
    path::PathBuf,
    time::Duration,
};

const HELP: &str = "DebugTUI 0.1.0 - native STM32 terminal debugger\n\nUsage: debugtui [--project debug.toml] [--elf firmware.elf] [options]\n\n  --tools-dir PATH       Bundled GDB / J-Link tools directory\n  --connect HOST:PORT    Use an already running GDB server\n  --port NUMBER          Managed server port (default 3333)\n  --device NAME          J-Link device (default STM32F429IG)\n  --log-dir PATH         Write GDB/MI and server session logs\n  --headless --stdio     JSON Lines automation (send connect to begin)\n  --script FILE          Execute JSON Lines; stop on error, then disconnect\n  --demo                 Explore the terminal UI without hardware\n  --snapshot FILE        Render a 120x36 demo UI to a text file\n  --version             Print version\n  --help                Print help\n\nKeys: F5 continue, F6 pause, F9 breakpoint, F10 next, F11 step,\n      Shift+F11 finish, Ctrl+P command palette, : command input, ? help.\n";
fn main() {
    if let Err(e) = run() {
        eprintln!("debugtui: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{HELP}");
        return Ok(());
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("debugtui {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let mut project = Project::default();
    let mut project_path = None;
    for (i, a) in args.iter().enumerate() {
        if a == "--project" {
            project_path = Some(args.get(i + 1).ok_or("--project requires a path")?);
        }
    }
    if let Some(path) = project_path {
        project = Project::load(&PathBuf::from(path))?;
    } else if PathBuf::from("debug.toml").is_file() {
        project = Project::load(&PathBuf::from("debug.toml"))?;
    }
    let mut headless = false;
    let mut script = None;
    let mut demo = false;
    let mut snapshot = None;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let mut value = || -> Result<String, String> {
            i += 1;
            args.get(i)
                .cloned()
                .ok_or_else(|| format!("{arg} requires a value"))
        };
        match arg.as_str() {
            "--project" => {
                value()?;
            }
            "--elf" => project.program.elf = value()?.into(),
            "--tools-dir" => project.tools.root = value()?.into(),
            "--connect" => {
                let address = value()?;
                let (host, port) = address
                    .rsplit_once(':')
                    .ok_or("--connect requires HOST:PORT")?;
                project.server.host = host.into();
                project.server.port = port.parse().map_err(|_| "Invalid port")?;
                project.server.mode = "external".into();
            }
            "--port" => project.server.port = value()?.parse().map_err(|_| "Invalid port")?,
            "--device" => project.server.device = value()?,
            "--log-dir" => project.session.log_dir = Some(value()?.into()),
            "--headless" | "--stdio" => headless = true,
            "--script" => {
                script = Some(value()?);
                headless = true;
            }
            "--demo" => demo = true,
            "--snapshot" => snapshot = Some(value()?),
            _ => return Err(format!("Unknown option: {arg}\nUse --help")),
        }
        i += 1;
    }
    project.validate()?;
    if let Some(path) = snapshot {
        return ui::snapshot(&PathBuf::from(path));
    }
    if headless {
        return run_headless(project, script);
    }
    ui::run(project, demo)
}

fn run_headless(project: Project, script: Option<String>) -> Result<(), String> {
    let engine = session::spawn(project);
    let (tx, rx) = std::sync::mpsc::channel::<Result<Request, String>>();
    let scripted = script.is_some();
    if let Some(path) = script {
        for (i, line) in fs::read_to_string(&path)
            .map_err(|e| format!("{path}: {e}"))?
            .trim_start_matches('\u{feff}')
            .lines()
            .enumerate()
        {
            if line.trim().is_empty() {
                continue;
            }
            tx.send(serde_json::from_str(line).map_err(|e| format!("Line {}: {e}", i + 1)))
                .map_err(|e| e.to_string())?;
        }
        drop(tx);
    } else {
        let cancellation = engine.cancellation.clone();
        std::thread::spawn(move || {
            for line in io::stdin().lock().lines() {
                match line {
                    Ok(line) if line.trim().is_empty() => continue,
                    Ok(line) => {
                        let request: Result<Request, String> =
                            serde_json::from_str(&line).map_err(|e| e.to_string());
                        if request.as_ref().is_ok_and(|r| r.method == "quit") {
                            cancellation.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        if tx.send(request).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }
    let mut pending = None;
    let mut ended = false;
    let mut failed = None;
    let mut quitting = false;
    loop {
        if pending.is_none() && !ended && !quitting {
            match rx.try_recv() {
                Ok(Ok(request)) => {
                    pending = Some(request.id);
                    quitting = request.method == "quit";
                    engine.send(request)?;
                }
                Ok(Err(e)) => {
                    eprintln!("{e}");
                    if scripted {
                        failed = Some(e);
                        ended = true;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => ended = true,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if ended && pending.is_none() && !quitting {
            quitting = true;
            engine.send(Request::new(u64::MAX, "quit", json!({})))?;
        }
        match engine.events.recv_timeout(Duration::from_millis(20)) {
            Ok(event) => {
                println!(
                    "{}",
                    serde_json::to_string(&event).map_err(|e| e.to_string())?
                );
                let _ = io::stdout().flush();
                if let Event::Response { id, ok, error, .. } = &event {
                    if Some(*id) == pending {
                        pending = None;
                        if !ok && scripted {
                            failed = error.clone();
                            ended = true;
                        }
                    }
                    if *id == u64::MAX && !ok {
                        failed = error.clone();
                    }
                }
                if matches!(event, Event::Exit) {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(
                    "Debug worker ended unexpectedly before session cleanup completed".into(),
                );
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
    if let Some(e) = failed { Err(e) } else { Ok(()) }
}
