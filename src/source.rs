//! Source paths come from the build host, not necessarily from this OS.
use crate::{
    config::{Project, SourceMap, portable_path},
    mi,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

pub(crate) fn normalized(path: &str) -> String {
    path.replace('\\', "/")
}

pub(crate) fn suffix<'a>(file: &'a str, from: &str) -> Option<&'a str> {
    let from = from.trim_end_matches('/');
    if from.is_empty() {
        // A POSIX root is a valid prefix, but an empty rule is not.
        return file.strip_prefix('/');
    }
    let tail = file.strip_prefix(from)?;
    if tail.is_empty() {
        Some(tail)
    } else {
        tail.strip_prefix('/')
    }
}

/// GDB on different hosts can retain either separator spelling from DWARF.
/// Keep the original spelling too, including mixed separators.
pub(crate) fn gdb_maps(maps: Vec<SourceMap>) -> Vec<SourceMap> {
    let mut seen = BTreeSet::new();
    let mut result = vec![];
    for map in maps {
        let forward = normalized(&map.from);
        for from in [map.from, forward.clone(), forward.replace('/', "\\")] {
            if !from.is_empty() && seen.insert(from.clone()) {
                result.push(SourceMap {
                    from,
                    to: map.to.clone(),
                });
            }
        }
    }
    result
}

#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub from: String,
    pub aliases: Vec<String>,
    pub files: usize,
    pub matched: usize,
    pub samples: Vec<(String, String, bool)>,
}

#[derive(Debug)]
pub(crate) struct Inventory {
    pub files: usize,
    pub candidates: Vec<Candidate>,
}

impl Inventory {
    fn from_files(files: Vec<String>, root: &Path, cancel: &AtomicBool) -> Result<Self, String> {
        let files: BTreeSet<_> = files
            .into_iter()
            .filter(|f| !f.chars().any(char::is_control))
            .filter(|f| {
                !matches!(
                    f.rsplit(['/', '\\']).next(),
                    Some("<built-in>" | "<command-line>" | "<artificial>")
                )
            })
            .collect();
        if files.is_empty() {
            return Err(
                "ELF has no source file records. Use an ELF with DWARF debug information.".into(),
            );
        }
        if files.len() > 50_000 {
            return Err("ELF source list exceeds 50000 files.".into());
        }
        let mut tree: BTreeMap<String, Candidate> = BTreeMap::new();
        let mut unique = BTreeSet::new();
        for raw in &files {
            if cancel.load(Ordering::Relaxed) {
                return Err("Scan cancelled".into());
            }
            let file = normalized(raw);
            let first_spelling = unique.insert(file.clone());
            for (i, _) in file.match_indices('/') {
                // Retain POSIX / and drive C:/ roots. Do not offer a partial UNC server.
                if file.starts_with("//") && (i <= 1 || file[2..i].find('/').is_none()) {
                    continue;
                }
                let end = if i == 0 || (i == 2 && file.as_bytes()[1] == b':') {
                    i + 1
                } else {
                    i
                };
                let from = &file[..end];
                let tail = &file[i + 1..];
                if tail.is_empty() {
                    continue;
                }
                let entry = tree.entry(from.into()).or_insert_with(|| Candidate {
                    from: from.into(),
                    aliases: vec![],
                    files: 0,
                    matched: 0,
                    samples: vec![],
                });
                let spelling = &raw[..end]; // '/' and '\\' have the same UTF-8 length.
                if spelling != from && !entry.aliases.iter().any(|a| a == spelling) {
                    entry.aliases.push(spelling.into());
                }
                if first_spelling {
                    let local = root.join(tail);
                    let exists = local.is_file();
                    entry.files += 1;
                    entry.matched += usize::from(exists);
                    if entry.samples.len() < 3 {
                        entry
                            .samples
                            .push((file.clone(), portable_path(&local), exists));
                    }
                }
            }
            if tree.len() > 50_000 {
                return Err("ELF directory tree exceeds 50000 entries.".into());
            }
        }
        if tree.is_empty() {
            return Err(
                "ELF lists bare file names only; no directory prefix is available to remap.".into(),
            );
        }
        Ok(Self {
            files: unique.len(),
            candidates: tree.into_values().collect(),
        })
    }
}

/// Dropping the receiver cancels the worker without blocking the terminal.
pub(crate) struct Scan {
    pub result: mpsc::Receiver<Result<Inventory, String>>,
    cancel: Arc<AtomicBool>,
}
impl Scan {
    pub fn start(project: Project) -> Result<Self, String> {
        if !project.program.elf.is_file() {
            return Err("Select an existing Program / ELF before scanning.".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let stopped = cancel.clone();
        let (tx, result) = mpsc::channel();
        thread::Builder::new()
            .name("elf-source-scan".into())
            .spawn(move || {
                let response = inspect(&project, &stopped).and_then(|files| {
                    Inventory::from_files(files, &project.program.source_root, &stopped)
                });
                let _ = tx.send(response);
            })
            .map_err(|e| format!("Start ELF scan: {e}"))?;
        Ok(Self { result, cancel })
    }
}
impl Drop for Scan {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn inspect(project: &Project, cancel: &AtomicBool) -> Result<Vec<String>, String> {
    let mut command = Command::new(&project.gdb.executable);
    // Never run profile args, init, services, or target commands here. Disable
    // all implicit init/ELF scripts before loading symbols in this offline GDB.
    command
        .args([
            "-nx",
            "-nh",
            "-q",
            "-iex",
            "set auto-load off",
            "-iex",
            "set source open off",
            "--interpreter=mi2",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(base) = project.path.as_deref().and_then(Path::parent) {
        command.current_dir(base);
    }
    crate::session::hidden(&mut command);
    crate::session::tool_environment(&mut command, &project.gdb.env, &project.gdb.unset_env);
    let job = crate::process::Job::new()?;
    let mut child = OwnedChild(
        command
            .spawn()
            .map_err(|e| format!("ELF scan GDB {}: {e}", project.gdb.executable.display()))?,
    );
    job.attach(&mut child.0)?;
    let mut input = child.0.stdin.take().ok_or("No GDB input")?;
    let stdout = child.0.stdout.take().ok_or("No GDB output")?;
    const MAX_OUTPUT: u64 = 32 * 1024 * 1024;
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("elf-source-output".into())
        .spawn(move || {
            let mut bytes = vec![];
            let result = stdout
                .take(MAX_OUTPUT + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
                .map_err(|e| format!("Read ELF metadata: {e}"));
            let _ = tx.send(result);
        })
        .map_err(|e| e.to_string())?;
    write!(
        input,
        "1-file-exec-and-symbols {}\n2-file-list-exec-source-files\n3-gdb-exit\n",
        mi::quote(&portable_path(&project.program.elf))
    )
    .map_err(|e| format!("Request ELF metadata: {e}"))?;
    drop(input);
    let deadline =
        Instant::now() + Duration::from_millis(project.session.timeout_ms.clamp(1000, 30000));
    let bytes = loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Scan cancelled".into());
        }
        if Instant::now() >= deadline {
            return Err("ELF scan timed out; GDB may be resolving an unavailable build/network path. Check GDB and the ELF. Press R to retry.".into());
        }
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(value) => break value?,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return Err("ELF scan output reader stopped".into()),
        }
    };
    if bytes.len() as u64 > MAX_OUTPUT {
        return Err("ELF metadata exceeds 32 MiB.".into());
    }
    let mut result = None;
    for line in String::from_utf8_lossy(&bytes).lines() {
        let Some(record) = mi::parse(line)? else {
            continue;
        };
        if record.kind != '^' {
            continue;
        }
        if record.class == "error" {
            return Err(format!("ELF scan: {}", record.data.string("msg")));
        }
        if record.token == Some(2) && record.class == "done" {
            let files = record
                .data
                .field("files")
                .ok_or("GDB returned no source file list")?;
            result = Some(
                files
                    .items()
                    .iter()
                    .map(|file| {
                        let full = file.string("fullname");
                        if full.is_empty() {
                            file.string("file")
                        } else {
                            full
                        }
                    })
                    .filter(|s| !s.is_empty())
                    .collect(),
            );
        }
    }
    result.ok_or_else(|| "GDB exited without returning ELF source metadata.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_spelling_precedence_and_source_root_preview() {
        let root = std::env::temp_dir().join(format!("debugtui-source-map-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/one.c"), "/* fixture */").unwrap();
        let inventory = Inventory::from_files(
            vec!["/ci/app/src/one.c".into(), "/ci/app/src/missing.c".into()],
            &root,
            &AtomicBool::new(false),
        )
        .unwrap();
        let c = inventory
            .candidates
            .iter()
            .find(|c| c.from == "/ci/app")
            .unwrap();
        assert_eq!((c.matched, c.files), (1, 2));
        let maps = gdb_maps(vec![SourceMap {
            from: r"C:\ci/app".into(),
            to: root.clone(),
        }]);
        assert_eq!(
            maps.iter().map(|m| m.from.as_str()).collect::<Vec<_>>(),
            [r"C:\ci/app", "C:/ci/app", r"C:\ci\app"]
        );
        assert!(
            Inventory::from_files(vec!["main.c".into()], &root, &AtomicBool::new(false))
                .unwrap_err()
                .contains("bare file names")
        );
        assert!(
            Inventory::from_files(vec![], &root, &AtomicBool::new(false))
                .unwrap_err()
                .contains("no source")
        );
        assert!(
            Inventory::from_files(vec!["/ci/app.c".into()], &root, &AtomicBool::new(true))
                .unwrap_err()
                .contains("cancelled")
        );
        std::fs::remove_file(root.join("src/one.c")).unwrap();
        std::fs::remove_dir(root.join("src")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn foreign_roots_boundaries_and_mixed_separators() {
        assert_eq!(suffix("/build/app/main.c", "/build/app"), Some("main.c"));
        assert_eq!(suffix("/build/app2/main.c", "/build/app"), None);
        assert_eq!(suffix("/main.c", "/"), Some("main.c"));
        assert_eq!(suffix("C:/main.c", "C:/"), Some("main.c"));
        let tree = Inventory::from_files(
            vec![
                r"C:\agent/project\src/main.c".into(),
                "C:/agent/project/src/main.c".into(),
                "//server/share/project/a.c".into(),
                "/build/工程 with spaces/src/x.c".into(),
            ],
            Path::new("missing-source-root"),
            &AtomicBool::new(false),
        )
        .unwrap();
        let candidate = tree
            .candidates
            .iter()
            .find(|c| c.from == "C:/agent/project")
            .unwrap();
        assert_eq!(candidate.files, 1);
        assert_eq!(candidate.aliases, [r"C:\agent/project"]);
        assert!(tree.candidates.iter().any(|c| c.from == "//server/share"));
        assert!(!tree.candidates.iter().any(|c| c.from == "//server"));
        assert!(
            tree.candidates
                .iter()
                .any(|c| c.from == "/build/工程 with spaces")
        );
        assert_eq!(tree.files, 3);
    }
}
