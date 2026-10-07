use debugtui::{
    config::Project,
    coordinator,
    session::{Event, Request, method},
    ui,
};
use serde_json::json;
use std::{
    env, fs,
    io::{self, BufRead, Write},
    path::PathBuf,
};

const HELP: &str = "DebugTUI - native GDB/MI terminal debugger\n\nUsage: debugtui [--project DIR|debug.toml] [options]\n\n  No arguments         Open launch setup; choose project and tools inside TUI\n  --setup              Review startup settings inside TUI before connecting\n  --project PATH       Project directory or configuration file\n  --gdb PATH            GDB executable (default: gdb on PATH)\n  --gdb-arg ARG         Additional GDB argument; may be repeated\n  --environment FILE    Optional tools environment profile\n  --tools-dir PATH      Shorthand for PATH/debug-env.toml\n  --connect ENDPOINT    Connect to an external GDB target; skip service launch\n  --target-mode MODE    remote, extended-remote or local\n  --local               Debug a local inferior; skip service launch\n  --chip NAME           Select a chip from the local device catalogue\n  --cores IDS           Comma-separated physical core IDs (e.g. 0,1)\n  --init-profiles       Create the chip catalogue and register extension directory; preserve customer files\n  --elf FILE            Optional executable/symbol file\n  --svd FILE            Optional CMSIS-SVD peripheral description\n  --log-dir PATH        Write session logs\n  --headless --stdio    JSON Lines automation\n  --script FILE         Execute JSON Lines and disconnect\n  --demo                Preview without GDB\n  --snapshot FILE       Render demo to a text file\n  --version             Print version\n  --help                Print help\n\nKeys: F2 launch setup, F5 continue/run, F6 pause, F9 breakpoint, F10 step over, F11 step in,\n      Shift+F11 step out, Ctrl+P help, Ctrl+Q exit.\n";
fn main() {
    if let Err(e) = run() {
        eprintln!("debugtui: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let options = debugtui::cli::Options::parse(env::args().skip(1))?;
    if options.help {
        print!("{HELP}");
        return Ok(());
    }
    if options.version {
        println!("debugtui {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if options.init_profiles {
        println!("{}", debugtui::devices::ensure_user_catalogue()?.display());
        return Ok(());
    }
    if let Some(path) = &options.snapshot {
        return ui::snapshot(&PathBuf::from(path));
    }
    let catalogue_error = debugtui::devices::ensure_user_catalogue().err();
    let (document, initial_error) = match options.document() {
        Ok(document) => (document, None),
        Err(e) if !options.headless => (
            debugtui::launch::Document::empty(options.project_path()),
            Some(e),
        ),
        Err(e) => return Err(e),
    };
    if options.headless {
        let mut project = document.project()?;
        if !document.path.is_file() {
            project.path = None;
        }
        return run_headless(project, options.script);
    }
    ui::run(
        document,
        options.demo,
        options.explicit_launch && !options.setup,
        initial_error.or(catalogue_error),
    )
}
fn run_headless(project: Project, script: Option<String>) -> Result<(), String> {
    // Requests and worker events both ring this bell; the loop sleeps between.
    let bell = debugtui::wake::Doorbell::default();
    let engine = coordinator::spawn_with(project, bell.clone());
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
        let canceller = engine.canceller();
        let bell = bell.clone();
        std::thread::spawn(move || {
            for line in io::stdin().lock().lines() {
                match line {
                    Ok(line) if line.trim().is_empty() => continue,
                    Ok(line) => {
                        let request: Result<Request, String> =
                            serde_json::from_str(&line).map_err(|e| e.to_string());
                        if request.as_ref().is_ok_and(Request::is_quit) {
                            canceller.cancel();
                        }
                        if tx.send(request).is_err() {
                            break;
                        }
                        bell.ring();
                    }
                    Err(_) => break,
                }
            }
            // End of input is news too: close the channel, then wake the loop.
            drop(tx);
            bell.ring();
        });
    }
    let mut pending = None;
    let mut ended = false;
    let mut failed = None;
    let mut quitting = false;
    loop {
        let mut progressed = false;
        if pending.is_none() && !ended && !quitting {
            match rx.try_recv() {
                Ok(Ok(request)) => {
                    progressed = true;
                    pending = Some(request.id);
                    quitting = request.is_quit();
                    engine.send(request)?;
                }
                Ok(Err(e)) => {
                    progressed = true;
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
            engine.send(Request::new(u64::MAX, method::QUIT, json!({})))?;
        }
        match engine.events.try_recv() {
            Ok(event) => {
                progressed = true;
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
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err(
                    "Debug worker ended unexpectedly before session cleanup completed".into(),
                );
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
        if !progressed {
            bell.wait(None);
        }
    }
    if let Some(e) = failed { Err(e) } else { Ok(()) }
}
