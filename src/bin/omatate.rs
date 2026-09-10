use omatate::core;
use serde_json::{Value, json};
use std::env;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime};
#[path = "../backend.rs"]
mod backend;
#[path = "../keyboard.rs"]
mod keyboard;
#[path = "../theme.rs"]
mod theme;

const PROMPT: &str = include_str!("../../share/analyze-prompt.md");
const SCHEMA: &str = include_str!("../../share/analyze-schema.json");

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("{message}");
    process::exit(1)
}
fn active(required: bool) -> Option<PathBuf> {
    let p = core::session_path();
    if required && p.is_none() {
        fail("inactive");
    }
    p
}

enum PanelError {
    Absent(String),
    Rejected(String),
    Failed(String),
}

impl fmt::Display for PanelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absent(error) | Self::Rejected(error) | Self::Failed(error) => f.write_str(error),
        }
    }
}

fn panel_request(cmd: &str, timeout: Duration) -> Result<Value, PanelError> {
    let mut stream = UnixStream::connect(core::runtime_dir().join("panel.sock"))
        .map_err(|e| PanelError::Absent(e.to_string()))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| PanelError::Failed(e.to_string()))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| PanelError::Failed(e.to_string()))?;
    writeln!(stream, "{}", json!({"cmd":cmd})).map_err(|e| PanelError::Failed(e.to_string()))?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|e| PanelError::Failed(e.to_string()))?;
    let reply: Value =
        serde_json::from_str(&line).map_err(|e| PanelError::Failed(e.to_string()))?;
    if reply.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(PanelError::Rejected(format!(
            "panel rejected {cmd}: {reply}"
        )));
    }
    Ok(reply)
}

fn launch_panel(request: &str) -> Result<(), String> {
    match panel_request(request, Duration::from_millis(250)) {
        Ok(_) => return Ok(()),
        Err(PanelError::Rejected(error)) => return Err(error),
        Err(_) => {}
    }
    let exe = env::current_exe()
        .ok()
        .and_then(|p| {
            let sibling = p.with_file_name("omatate-panel");
            sibling.is_file().then_some(sibling)
        })
        .or_else(|| find_program("omatate-panel"))
        .ok_or("cannot find omatate-panel")?;
    let mut c = Command::new(exe);
    c.arg(request)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    unsafe {
        c.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let output = c
        .output()
        .map_err(|e| format!("cannot launch panel: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot open Omatate plugin: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

fn find_program(name: &str) -> Option<PathBuf> {
    env::split_paths(&env::var_os("PATH")?)
        .map(|p| p.join(name))
        .find(|p| p.is_file())
}
fn canonical(path: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(path);
    let p = if p.starts_with("~") {
        core::home().join(p.strip_prefix("~").unwrap())
    } else {
        p
    };
    p.canonicalize().map_err(|e| e.to_string())
}
fn expanded(path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    if p.starts_with("~") {
        core::home().join(p.strip_prefix("~").unwrap())
    } else {
        p
    }
}
fn resolved_loose(path: &str) -> Result<PathBuf, String> {
    let p = expanded(path);
    if let Ok(v) = p.canonicalize() {
        return Ok(v);
    }
    if p.is_absolute() {
        return Ok(p);
    }
    env::current_dir()
        .map(|cwd| cwd.join(p))
        .map_err(|e| e.to_string())
}
fn pointer_text() -> String {
    fs::read_to_string(core::runtime_dir().join("session"))
        .unwrap_or_default()
        .trim_end_matches(['\n', '\r'])
        .to_owned()
}
fn validate(session: &Path) -> Result<(), String> {
    if !session.is_dir() || !session.join(".data/entries.jsonl").is_file() {
        return Err("missing .data/entries.jsonl".into());
    }
    if fs::read_to_string(session.join(".data/id"))
        .map_err(|e| e.to_string())?
        .trim()
        .is_empty()
    {
        return Err("empty .data/id".into());
    }
    for e in core::read_entries(session)? {
        if !e.is_object() || e.get("id").and_then(Value::as_i64).is_none_or(|n| n < 1) {
            return Err("entries must have positive integer ids".into());
        }
    }
    Ok(())
}
fn registry() -> PathBuf {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| core::home().join(".local/state"))
        .join("omatate/projects.json")
}
fn read_registry() -> Result<Vec<String>, String> {
    let p = registry();
    if !p.exists() {
        return Ok(vec![]);
    }
    let v: Value = serde_json::from_str(&fs::read_to_string(p).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let a = v
        .as_array()
        .ok_or("projects.json must contain an array of absolute project paths")?;
    a.iter()
        .map(|v| {
            v.as_str()
                .filter(|s| Path::new(s).is_absolute())
                .map(str::to_owned)
                .ok_or_else(|| {
                    "projects.json must contain an array of absolute project paths".into()
                })
        })
        .collect()
}
fn remember(session: &Path) {
    let result = (|| {
        let mut paths = read_registry()?;
        let s = session
            .canonicalize()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned();
        paths.retain(|p| p != &s);
        paths.insert(0, s);
        let path = registry();
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        core::atomic_write(
            &path,
            format!("{}\n", serde_json::to_string(&paths).unwrap()).as_bytes(),
        )
    })();
    if let Err(e) = result {
        eprintln!("omatate: cannot remember project: {e}")
    }
}
fn restore_last_project() {
    if active(false).is_some() {
        return;
    }
    let _ = core::with_lock(|| {
        if active(false).is_some() {
            return Ok(());
        }
        let Some(last) = read_registry()?.into_iter().next() else {
            return Ok(());
        };
        let session = PathBuf::from(last)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        validate(&session)?;
        if !session.join("notes.md").is_file() {
            return Err("existing session is missing notes.md".into());
        }
        match fs::read_to_string(session.join(".data/draft.txt")) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
        install_pointer(&session, None)
    });
}

fn launch_with_last_project(request: &str) -> Result<(), String> {
    match panel_request(request, Duration::from_millis(250)) {
        Ok(_) => return Ok(()),
        Err(PanelError::Rejected(error)) => return Err(error),
        Err(_) => {}
    }
    restore_last_project();
    launch_panel(request)
}
fn install_pointer(session: &Path, expected: Option<&str>) -> Result<(), String> {
    let current = pointer_text();
    let normalized = if current.is_empty() {
        String::new()
    } else {
        Path::new(&current)
            .canonicalize()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned()
    };
    if let Some(exp) = expected {
        let exp = if exp.is_empty() {
            String::new()
        } else {
            resolved_loose(exp)?.to_string_lossy().into_owned()
        };
        if normalized != exp {
            return Err("active project changed; reload projects and try again".into());
        }
    } else if !normalized.is_empty() && Path::new(&normalized) != session {
        return Err("another session is active; stop it before opening this project".into());
    }
    fs::create_dir_all(core::runtime_dir()).map_err(|e| e.to_string())?;
    core::atomic_write(
        &core::runtime_dir().join("session"),
        format!("{}\n", session.display()).as_bytes(),
    )
}
fn create_layout(session: &Path) -> Result<(), String> {
    let data = session.join(".data");
    fs::create_dir(&data).map_err(|e| e.to_string())?;
    for n in ["transcripts", "context", "logs"] {
        fs::create_dir(data.join(n)).map_err(|e| e.to_string())?
    }
    fs::write(data.join("entries.jsonl"), b"").map_err(|e| e.to_string())?;
    let id = format!(
        "{:x}{:x}\n",
        unsafe { libc::getpid() },
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    fs::write(data.join("id"), id).map_err(|e| e.to_string())?;
    core::render(session)
}
fn valid_project_name(n: &str) -> bool {
    !n.is_empty() && n.trim() == n && !matches!(n, "." | "..") && !n.contains(['/', '\\', '\0'])
}

fn open_project(base: &str, expected: Option<&str>, name: Option<&str>) -> Result<PathBuf, String> {
    let raw = expanded(base);
    if let Some(n) = name {
        if !valid_project_name(n) {
            return Err("project name must be a single nonempty folder name".into());
        }
        if !raw.is_dir() {
            return Err("choose an existing parent directory".into());
        }
    } else if !raw.is_dir() {
        return Err("choose an existing project directory".into());
    }
    let mut session = raw.canonicalize().map_err(|e| e.to_string())?;
    if let Some(n) = name {
        session = session.join(n)
    }
    core::with_lock(|| {
        let current = pointer_text();
        let cur = if current.is_empty() {
            None
        } else {
            Some(
                PathBuf::from(&current)
                    .canonicalize()
                    .map_err(|e| e.to_string())?,
            )
        };
        if let Some(exp) = expected {
            let exp = if exp.is_empty() {
                None
            } else {
                Some(resolved_loose(exp)?)
            };
            if cur != exp {
                return Err("active project changed; reload projects and try again".into());
            }
        } else if cur.as_ref().is_some_and(|p| p != &session) {
            return Err("another session is active; stop it before opening this project".into());
        }
        let outgoing = if expected.is_some() && cur.as_ref().is_some_and(|p| p != &session) {
            let p = cur.as_ref().unwrap();
            let e = core::read_entries(p)?;
            if e.iter().any(|v| {
                matches!(
                    v.get("status").and_then(Value::as_str),
                    Some("recording" | "transcribing")
                )
            }) {
                return Err("finish recording and transcription before switching projects".into());
            }
            Some((p.clone(), e))
        } else {
            None
        };
        let mut made_dir = false;
        let mut made_data = false;
        let result = (|| {
            if name.is_some() {
                if session.exists() || session.is_symlink() {
                    return Err("project folder already exists; choose another name".into());
                }
                fs::create_dir(&session).map_err(|e| e.to_string())?;
                made_dir = true
            }
            let data = session.join(".data");
            if data.exists() || data.is_symlink() {
                validate(&session)?;
                match fs::read_to_string(data.join("draft.txt")) {
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.to_string()),
                }
                if !session.join("notes.md").is_file() {
                    return Err("existing session is missing notes.md".into());
                }
            } else {
                if session.join("notes.md").exists() || session.join("notes.md").is_symlink() {
                    return Err(
                        "project already contains notes.md without a valid notes session".into(),
                    );
                }
                made_data = true;
                create_layout(&session)?;
            }
            if let Some((p, e)) = &outgoing {
                core::render_entries(p, e)?
            }
            install_pointer(&session, expected)?;
            if let Some((p, _)) = &outgoing {
                remember(p)
            }
            remember(&session);
            Ok(())
        })();
        if result.is_err() {
            if made_data {
                let _ = fs::remove_dir_all(session.join(".data"));
                let _ = fs::remove_file(session.join("notes.md"));
            }
            if made_dir {
                let _ = fs::remove_dir(&session);
            }
        }
        result
    })?;
    Ok(session)
}

fn project_paths() -> Result<Vec<String>, String> {
    let mut candidates = active(false)
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    match read_registry() {
        Ok(v) => candidates.extend(v),
        Err(e) => eprintln!("omatate: cannot read project list: {e}"),
    }
    let legacy = core::home().join("Documents/omatate");
    if let Ok(rd) = fs::read_dir(legacy) {
        let mut v = rd
            .filter_map(Result::ok)
            .map(|e| e.path())
            .collect::<Vec<_>>();
        v.sort();
        candidates.extend(v.into_iter().map(|p| p.to_string_lossy().into_owned()));
    }
    let mut out = vec![];
    for s in candidates {
        if let Ok(p) = PathBuf::from(s).canonicalize() {
            let text = p.to_string_lossy().into_owned();
            if !out.contains(&text) && validate(&p).is_ok() {
                out.push(text)
            }
        }
    }
    Ok(out)
}
fn projects() -> Result<(), String> {
    core::with_lock(|| {
        println!("{}", serde_json::to_string(&project_paths()?).unwrap());
        Ok(())
    })
}
fn clean_session_name(name: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
            out.push(c);
            dash = false
        } else if !dash {
            out.push('-');
            dash = true
        }
    }
    let out = out.trim_matches(['-', '.']).to_owned();
    if out.is_empty() {
        Err("session name contains no usable characters".into())
    } else {
        Ok(out)
    }
}
fn remove_tree_contents(path: &Path) -> Result<(), String> {
    if !path.exists() {
        fs::create_dir_all(path).map_err(|e| e.to_string())?;
        return Ok(());
    }
    for item in fs::read_dir(path).map_err(|e| e.to_string())? {
        let p = item.map_err(|e| e.to_string())?.path();
        if p.is_dir() && !p.is_symlink() {
            fs::remove_dir_all(p).map_err(|e| e.to_string())?
        } else {
            fs::remove_file(p).map_err(|e| e.to_string())?
        }
    }
    Ok(())
}
fn start(name: Option<&str>) -> Result<PathBuf, String> {
    let session = core::with_lock(|| {
        if let Some(s) = active(false) {
            remember(&s);
            return Ok(s);
        }
        remove_tree_contents(&core::runtime_dir().join("shots"))?;
        let root = core::home().join("Documents/omatate");
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let stamp = core::filename_timestamp();
        let suffix = match name {
            Some(n) => format!("-{}", clean_session_name(n)?),
            None => String::new(),
        };
        let mut session = root.join(format!("{stamp}{suffix}"));
        let mut counter = 2;
        while session.exists() {
            session = root.join(format!("{stamp}{suffix}-{counter}"));
            counter += 1
        }
        fs::create_dir(&session).map_err(|e| e.to_string())?;
        if let Err(e) = create_layout(&session).and_then(|_| install_pointer(&session, None)) {
            let _ = fs::remove_dir_all(&session);
            return Err(e);
        }
        remember(&session);
        Ok(session)
    })?;
    launch_panel("ping")?;
    Ok(session)
}
fn remove_entry(session: &Path, id: i64) {
    let _ = core::with_lock(|| {
        let mut entries = core::read_entries(session)?;
        entries.retain(|v| v.get("id").and_then(Value::as_i64) != Some(id));
        core::write_entries(session, &entries)?;
        core::render_entries(session, &entries)
    });
}

fn clip() -> Result<(), String> {
    let session = active(true).unwrap();
    let session_id = fs::read_to_string(session.join(".data/id"))
        .map_err(|e| e.to_string())?
        .trim()
        .to_owned();
    core::with_lock(|| {
        if core::session_path().as_deref() != Some(session.as_path()) {
            return Err("active project changed before capture".into());
        }
        if core::read_entries(&session)?.iter().any(|entry| {
            matches!(
                entry.get("status").and_then(Value::as_str),
                Some("recording" | "transcribing")
            )
        }) {
            return Err("finish recording and transcription before capturing".into());
        }
        Ok(())
    })?;

    panel_request("hide", Duration::from_millis(1500))
        .map_err(|e| format!("cannot hide panel for capture: {e}"))?;
    thread::sleep(Duration::from_millis(50));
    let capture = (|| {
        let selection = Command::new("slurp")
            .output()
            .map_err(|e| format!("cannot run slurp: {e}"))?;
        let region = String::from_utf8_lossy(&selection.stdout).trim().to_owned();
        if !selection.status.success() {
            let error = String::from_utf8_lossy(&selection.stderr).trim().to_owned();
            if !error.is_empty() && !error.contains("selection cancelled") {
                return Err(format!("slurp failed: {error}"));
            }
            return Ok(None);
        }
        if region.is_empty() {
            return Ok(None);
        }
        let (width, height) = region
            .split_once(' ')
            .and_then(|(_, size)| size.split_once('x'))
            .and_then(|(width, height)| {
                Some((width.parse::<i32>().ok()?, height.parse::<i32>().ok()?))
            })
            .filter(|(width, height)| *width > 0 && *height > 0)
            .ok_or("slurp returned invalid capture dimensions")?;
        let captures = core::runtime_dir().join("captures");
        fs::create_dir_all(&captures).map_err(|e| e.to_string())?;
        let asset = captures.join(format!(
            "clip-{}-{}.png",
            process::id(),
            core::filename_timestamp()
        ));
        let status = Command::new("grim")
            .args(["-g", &region])
            .arg(&asset)
            .status()
            .map_err(|e| e.to_string());
        if !status.is_ok_and(|value| value.success()) {
            let _ = fs::remove_file(&asset);
            return Err("grim failed".into());
        }
        Ok(Some((asset, width, height)))
    })();
    let restore = panel_request("show", Duration::from_millis(1500));
    if let Err(error) = restore {
        if let Ok(Some((asset, _, _))) = &capture {
            let _ = fs::remove_file(asset);
        }
        return Err(format!("cannot restore panel after capture: {error}"));
    }
    let Some((captured, width, height)) = capture? else {
        return Ok(());
    };

    let committed = core::with_lock(|| {
        if core::session_path().as_deref() != Some(session.as_path())
            || fs::read_to_string(session.join(".data/id"))
                .ok()
                .is_none_or(|id| id.trim() != session_id)
        {
            return Err("active project changed during capture".into());
        }
        let mut entries = core::read_entries(&session)?;
        if entries.iter().any(|entry| {
            matches!(
                entry.get("status").and_then(Value::as_str),
                Some("recording" | "transcribing")
            )
        }) {
            return Err("recording started during capture".into());
        }
        let id = entries
            .iter()
            .filter_map(|entry| entry.get("id").and_then(Value::as_i64))
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("entry ids exhausted")?;
        let assets = session.join("assets");
        if assets.is_symlink() {
            return Err("project assets must be a directory inside the project".into());
        }
        fs::create_dir_all(&assets).map_err(|e| e.to_string())?;
        let stamp = core::filename_timestamp();
        let (asset, relative) = (0..10_000)
            .find_map(|counter| {
                let name = if counter == 0 {
                    format!("clip-{stamp}.png")
                } else {
                    format!("clip-{stamp}-{counter}.png")
                };
                let path = assets.join(&name);
                match OpenOptions::new().write(true).create_new(true).open(&path) {
                    Ok(_) => Some(Ok((path, format!("assets/{name}")))),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => None,
                    Err(e) => Some(Err(e.to_string())),
                }
            })
            .transpose()?
            .ok_or("cannot allocate a unique capture filename")?;
        if let Err(error) = fs::copy(&captured, &asset) {
            let _ = fs::remove_file(&asset);
            return Err(error.to_string());
        }
        let original = entries.clone();
        entries.push(json!({"id":id,"ts":core::timestamp(),"ai":false,"status":"done","transcript":"","asset":relative,"asset_size":{"width":width,"height":height}}));
        if let Err(error) = core::write_entries(&session, &entries) {
            let _ = fs::remove_file(&asset);
            return Err(error);
        }
        if let Err(error) = core::render_entries(&session, &entries) {
            if core::write_entries(&session, &original).is_ok() {
                let _ = core::render_entries(&session, &original);
                let _ = fs::remove_file(&asset);
            }
            return Err(error);
        }
        Ok(id)
    });
    let _ = fs::remove_file(&captured);
    let id = committed?;
    let _ = panel_request("reload", Duration::from_secs(1));
    println!("{id}");
    Ok(())
}
fn spawn_worker(action: &str, id: i64, session: &Path) -> Result<(), String> {
    let exe = env::current_exe().map_err(|e| e.to_string())?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(session.join(format!(".data/logs/{action}-{id:03}.log")))
        .map_err(|e| e.to_string())?;
    let log2 = log.try_clone().map_err(|e| e.to_string())?;
    let mut c = Command::new(exe);
    c.args([action, &id.to_string()])
        .env("OMATATE_SESSION_PATH", session)
        .env(
            "OMATATE_SESSION_ID",
            fs::read_to_string(session.join(".data/id"))
                .unwrap_or_default()
                .trim(),
        )
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(log2);
    unsafe {
        c.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    c.spawn().map_err(|e| e.to_string())?;
    Ok(())
}
fn exec_voxtype(action: &str) -> ! {
    let e = Command::new("voxtype").args(["record", action]).exec();
    fail(e)
}

fn ptt_start() -> Result<(), String> {
    let session = active(true).unwrap();
    let focus = panel_request("focus", Duration::from_millis(300)).ok();
    if !focus.as_ref().is_some_and(|v| {
        v.get("active") == Some(&Value::Bool(true))
            && v.get("focus").and_then(Value::as_str) == Some("note")
    }) {
        exec_voxtype("start")
    };
    let ai = core::ai_enabled();
    let session_id = fs::read_to_string(session.join(".data/id"))
        .map_err(|e| e.to_string())?
        .trim()
        .to_owned();
    let id = core::with_lock(|| {
        if core::session_path().as_deref() != Some(session.as_path()) {
            return Err("active project changed".into());
        }
        let entries = core::read_entries(&session)?;
        let id = entries
            .iter()
            .filter_map(|v| v.get("id").and_then(Value::as_i64))
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("entry ids exhausted")?;
        let entry = if ai {
            json!({"id":id,"ts":core::timestamp(),"status":"recording","context_status":"pending"})
        } else {
            json!({"id":id,"ts":core::timestamp(),"ai":false,"status":"recording"})
        };
        core::rewrite_entry(&session, id, entry)?;
        Ok(id)
    })?;
    let transcript = session.join(format!(".data/transcripts/{id:03}.txt"));
    let status = Command::new("voxtype")
        .args([
            "record",
            "start",
            &format!("--file={}", transcript.display()),
        ])
        .status()
        .map_err(|e| {
            remove_entry(&session, id);
            e.to_string()
        })?;
    if !status.success() {
        remove_entry(&session, id);
        return Err(format!("voxtype exited with status {status}"));
    }
    if !ai {
        let _ = panel_request("reload", Duration::from_millis(1500));
        return Ok(());
    }
    let shot = core::runtime_dir()
        .join("shots")
        .join(&session_id)
        .join(format!("{id:03}.png"));
    let result = (|| {
        fs::create_dir_all(shot.parent().unwrap()).map_err(|e| e.to_string())?;
        let raw = Command::new("hyprctl")
            .args(["monitors", "-j"])
            .output()
            .map_err(|e| e.to_string())?;
        let mons: Value = serde_json::from_slice(&raw.stdout).map_err(|e| e.to_string())?;
        let a = mons.as_array().ok_or("invalid monitor data")?;
        let mon = a
            .iter()
            .find(|v| v.get("focused") == Some(&Value::Bool(true)))
            .or(a.first())
            .and_then(|v| v.get("name"))
            .and_then(Value::as_str)
            .ok_or("no monitor")?;
        let _ = panel_request("hide", Duration::from_millis(1500));
        thread::sleep(Duration::from_millis(50));
        let grim = Command::new("grim")
            .args(["-o", mon])
            .arg(&shot)
            .status()
            .map_err(|e| e.to_string());
        let _ = panel_request("show", Duration::from_millis(1500));
        if !grim?.success() {
            return Err("grim failed".into());
        }
        let window = Command::new("hyprctl")
            .args(["activewindow", "-j"])
            .output()
            .ok()
            .and_then(|o| serde_json::from_slice::<Value>(&o.stdout).ok())
            .unwrap_or(json!({}));
        core::with_lock(|| {
            core::update_entry(&session, id, |v| {
                v["shot"] = json!(shot);
                v["window"] = json!({"class":window.get("class").and_then(Value::as_str).unwrap_or(""),"title":window.get("title").and_then(Value::as_str).unwrap_or("")});
                Ok(())
            })
        })
    })();
    if let Err(e) = result {
        let _ = Command::new("voxtype").args(["record", "cancel"]).status();
        remove_entry(&session, id);
        return Err(e);
    }
    let _ = panel_request("reload", Duration::from_millis(1500));
    if let Err(error) = spawn_worker("analyze", id, &session) {
        schedule_cleanup(&shot);
        core::with_lock(|| {
            core::update_entry(&session, id, |v| {
                v["context_status"] = json!("error");
                Ok(())
            })
        })?;
        return Err(error);
    }
    Ok(())
}

fn ptt_stop() -> Result<(), String> {
    let session = active(true).unwrap();
    let entries = core::read_entries(&session)?;
    let ids = entries
        .iter()
        .filter(|v| {
            v.get("kind").is_none() && v.get("status").and_then(Value::as_str) == Some("recording")
        })
        .filter_map(|v| v.get("id").and_then(Value::as_i64))
        .collect::<Vec<_>>();
    if ids.is_empty() {
        exec_voxtype("stop")
    }
    let id = *ids.iter().max().unwrap();
    let status = Command::new("voxtype").args(["record", "stop"]).status();
    if !status.as_ref().is_ok_and(|s| s.success()) {
        core::with_lock(|| {
            core::update_entry(&session, id, |v| {
                v["status"] = json!("no-transcript");
                Ok(())
            })
        })?;
        let _ = panel_request("reload", Duration::from_secs(1));
        return Err("omatate: voxtype record stop failed".into());
    }
    core::with_lock(|| {
        core::update_entry(&session, id, |v| {
            v["status"] = json!("transcribing");
            Ok(())
        })
    })?;
    let _ = panel_request("reload", Duration::from_secs(1));
    if let Err(error) = spawn_worker("ingest", id, &session) {
        core::with_lock(|| {
            core::update_entry(&session, id, |v| {
                v["status"] = json!("no-transcript");
                Ok(())
            })
        })?;
        let _ = panel_request("reload", Duration::from_secs(1));
        return Err(error);
    }
    Ok(())
}

fn valid_context(v: &Value) -> bool {
    let Some(o) = v.as_object() else { return false };
    if o.len() != 4
        || !["title", "summary", "regions", "notable"]
            .iter()
            .all(|k| o.contains_key(*k))
    {
        return false;
    }
    if !o["title"].is_string()
        || !o["summary"].is_string()
        || !o["notable"]
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string))
    {
        return false;
    }
    o["regions"].as_array().is_some_and(|a| {
        a.iter().all(|r| {
            r.as_object().is_some_and(|m| {
                m.len() == 2
                    && m.get("name").is_some_and(Value::is_string)
                    && m.get("contents").is_some_and(Value::is_string)
            })
        })
    })
}
fn extract_context(s: &str) -> Result<Value, String> {
    if let Ok(v) = serde_json::from_str::<Value>(s)
        && valid_context(&v)
    {
        return Ok(v);
    }
    for (i, c) in s.char_indices() {
        if c == '{' {
            let mut stream = serde_json::Deserializer::from_str(&s[i..]).into_iter::<Value>();
            if let Some(Ok(v)) = stream.next()
                && valid_context(&v)
            {
                return Ok(v);
            }
        }
    }
    Err("could not parse Codex output as context JSON".into())
}
fn analyze(id: i64) -> Result<(), String> {
    let session = active(true).unwrap();
    let entries = core::read_entries(&session)?;
    let found = entries
        .iter()
        .find(|v| v.get("id").and_then(Value::as_i64) == Some(id));
    let Some(found) = found else {
        return Err(format!("entry {id} not found"));
    };
    if found.get("kind").and_then(Value::as_str) == Some("section") {
        return Err(format!("entry {id} is a section"));
    }
    if found.get("ai").and_then(Value::as_bool) == Some(false) {
        return Err(format!("entry {id} is a plain entry"));
    }
    let token = fs::read_to_string(session.join(".data/id")).unwrap_or_else(|_| "unknown".into());
    let shot = found
        .get("shot")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            core::runtime_dir()
                .join("shots")
                .join(token.trim())
                .join(format!("{id:03}.png"))
        });
    let worker_dir = core::runtime_dir().join("workers");
    fs::create_dir_all(&worker_dir).map_err(|e| e.to_string())?;
    let token_key = token
        .trim()
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let stem = format!("{token_key}-{id:03}");
    let output = worker_dir.join(format!("{stem}.json"));
    let schema = worker_dir.join(format!("{stem}.schema.json"));
    let result: Result<Value, String> = (|| {
        core::atomic_write(&schema, SCHEMA.as_bytes())?;
        let mut c = Command::new("codex");
        c.args([
            "exec",
            "--skip-git-repo-check",
            "--sandbox",
            "read-only",
            "-C",
        ])
        .arg(&session)
        .args(["-i"])
        .arg(&shot)
        .args(["--output-schema"])
        .arg(&schema)
        .args(["-o"])
        .arg(&output)
        .args([
            "-c",
            &format!(
                "model_reasoning_effort=\"{}\"",
                env::var("OMATATE_REASONING").unwrap_or_else(|_| "low".into())
            ),
        ]);
        let model = env::var("OMATATE_MODEL")
            .ok()
            .filter(|model| !model.is_empty())
            .unwrap_or_else(|| "gpt-5.6-sol".into());
        c.args(["-m", &model]);
        c.arg("-").stdin(Stdio::piped()).process_group(0);
        let mut child = c.spawn().map_err(|e| e.to_string())?;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(PROMPT.as_bytes())
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(240);
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break status;
            }
            if Instant::now() >= deadline {
                unsafe {
                    libc::killpg(child.id() as i32, libc::SIGKILL);
                }
                let _ = child.wait();
                break process::ExitStatus::from_raw(124 << 8);
            }
            thread::sleep(Duration::from_millis(100));
        };
        if status.success() {
            fs::read_to_string(&output)
                .map_err(|e| e.to_string())
                .and_then(|s| extract_context(&s))
        } else {
            Err(format!("codex exited with status {status}"))
        }
    })();
    let _ = fs::remove_file(&schema);
    let _ = fs::remove_file(&output);
    let relocated = core::session_path().unwrap_or(session);
    let relocated_output = relocated.join(format!(".data/context/{id:03}.json"));
    schedule_cleanup(&shot);
    match result {
        Ok(ctx) => {
            core::atomic_write(
                &relocated_output,
                format!("{}\n", serde_json::to_string_pretty(&ctx).unwrap()).as_bytes(),
            )?;
            core::with_lock(|| {
                core::update_entry(&relocated, id, |v| {
                    v["context"] = ctx;
                    v["context_status"] = json!("done");
                    Ok(())
                })
            })?;
        }
        Err(e) => {
            core::with_lock(|| {
                core::update_entry(&relocated, id, |v| {
                    if let Some(o) = v.as_object_mut() {
                        o.remove("context");
                    }
                    v["context_status"] = json!("error");
                    Ok(())
                })
            })?;
            let _ = panel_request("reload", Duration::from_secs(1));
            return Err(e);
        }
    }
    let _ = panel_request("reload", Duration::from_secs(1));
    Ok(())
}
fn schedule_cleanup(shot: &Path) {
    let Ok(exe) = env::current_exe() else {
        cleanup_shot(shot);
        return;
    };
    let mut c = Command::new(exe);
    c.arg("cleanup-shot")
        .arg(shot)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        c.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    if c.spawn().is_err() {
        cleanup_shot(shot);
    }
}
fn cleanup_shot(shot: &Path) {
    let age = fs::metadata(shot)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|m| SystemTime::now().duration_since(m).ok())
        .unwrap_or(Duration::from_secs(300));
    if age < Duration::from_secs(300) {
        thread::sleep(Duration::from_secs(300) - age)
    }
    let _ = fs::remove_file(shot);
    if let Some(parent) = shot.parent() {
        let _ = fs::remove_dir(parent);
    }
}
fn ingest(id: i64) -> Result<(), String> {
    let session = active(true).unwrap();
    if core::read_entries(&session)?
        .iter()
        .find(|v| v.get("id").and_then(Value::as_i64) == Some(id))
        .and_then(|v| v.get("kind"))
        .is_some()
    {
        return Err(format!("entry {id} is a section"));
    }
    let state = match env::var_os("XDG_RUNTIME_DIR") {
        Some(base) => PathBuf::from(base).join("voxtype/state"),
        None => PathBuf::from(format!("/tmp/voxtype-{}/state", unsafe { libc::getuid() })),
    };
    let start = Instant::now();
    let mut idle = None;
    let mut text = None;
    let branch = loop {
        let current_session = core::session_path().unwrap_or_else(|| session.clone());
        let candidate =
            fs::read_to_string(current_session.join(format!(".data/transcripts/{id:03}.txt")))
                .unwrap_or_default()
                .trim()
                .to_owned();
        let is_idle = fs::read_to_string(&state)
            .ok()
            .is_some_and(|s| s.trim() == "idle");
        if !candidate.is_empty() && is_idle {
            text = Some(candidate);
            break "success";
        }
        if is_idle {
            let since = *idle.get_or_insert_with(Instant::now);
            if candidate.is_empty()
                && since.elapsed() >= Duration::from_millis(1500)
                && start.elapsed() >= Duration::from_secs(3)
            {
                break "idle-without-transcript";
            }
        } else {
            idle = None
        }
        if start.elapsed() >= Duration::from_secs(120) {
            break "timeout";
        }
        thread::sleep(Duration::from_millis(200))
    };
    let relocated = core::session_path().unwrap_or(session);
    writeln!(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(relocated.join(format!(".data/logs/ingest-{id:03}.log")))
            .map_err(|e| e.to_string())?,
        "omatate ingest: {branch} after {:.2}s",
        start.elapsed().as_secs_f64()
    )
    .map_err(|e| e.to_string())?;
    core::with_lock(|| {
        core::update_entry(&relocated, id, |v| {
            if let Some(t) = text {
                v["transcript"] = json!(t);
                v["status"] = json!("done")
            } else {
                v["status"] = json!("no-transcript")
            }
            Ok(())
        })
    })?;
    let _ = panel_request("reload", Duration::from_secs(1));
    Ok(())
}

fn stop() -> Result<(), String> {
    let stopped = active(false);
    let panel = match panel_request("quit", Duration::from_millis(1500)) {
        Ok(v) => Some(v),
        Err(e @ PanelError::Rejected(_)) => {
            return Err(format!("cannot stop session: {e}"));
        }
        Err(PanelError::Absent(_)) => None,
        Err(e @ PanelError::Failed(_)) => {
            return Err(format!("cannot stop session: {e}"));
        }
    };
    let mut render_failed = false;
    core::with_lock(|| {
        if pointer_text()
            != stopped
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
        {
            return Ok(());
        }
        if panel.is_some()
            && let Some(s) = &stopped
            && let Err(e) = core::render(s)
        {
            render_failed = true;
            eprintln!("omatate: cannot render stopped session: {e}")
        }
        match fs::remove_file(core::runtime_dir().join("session")) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
        let Some(s) = &stopped else { return Ok(()) };
        if render_failed {
            return Ok(());
        }
        let root = core::home().join("Documents/omatate");
        let named = s.parent() == Some(root.as_path())
            && s.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.len() >= 15
                    && n.as_bytes()[..8].iter().all(u8::is_ascii_digit)
                    && n.as_bytes()[8] == b'-'
                    && n.as_bytes()[9..15].iter().all(u8::is_ascii_digit)
            });
        if named && generated_session_is_empty(s) {
            let _ = fs::remove_dir_all(s);
        }
        Ok(())
    })?;
    Ok(())
}
fn dir_has_no_files(p: &Path) -> bool {
    let Ok(rd) = fs::read_dir(p) else {
        return false;
    };
    for entry in rd {
        let Ok(entry) = entry else { return false };
        if !entry.file_type().ok().is_some_and(|kind| kind.is_dir())
            || !dir_has_no_files(&entry.path())
        {
            return false;
        }
    }
    true
}
fn generated_session_is_empty(session: &Path) -> bool {
    if !session
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_dir())
    {
        return false;
    }
    let Ok(items) = fs::read_dir(session) else {
        return false;
    };
    for item in items {
        let Ok(item) = item else { return false };
        let Some(name) = item.file_name().to_str().map(str::to_owned) else {
            return false;
        };
        let Ok(kind) = item.file_type() else {
            return false;
        };
        match name.as_str() {
            "notes.md" if kind.is_file() => {}
            ".data" if kind.is_dir() => {}
            "assets" if kind.is_dir() && dir_has_no_files(&item.path()) => {}
            _ => return false,
        }
    }
    if !session.join("notes.md").is_file() || !session.join(".data").is_dir() {
        return false;
    }
    let data = session.join(".data");
    let Ok(items) = fs::read_dir(&data) else {
        return false;
    };
    for item in items {
        let Ok(item) = item else { return false };
        let Some(name) = item.file_name().to_str().map(str::to_owned) else {
            return false;
        };
        let Ok(kind) = item.file_type() else {
            return false;
        };
        match name.as_str() {
            "entries.jsonl" if kind.is_file() => {
                if !fs::read_to_string(item.path()).is_ok_and(|v| v.is_empty()) {
                    return false;
                }
            }
            "id" if kind.is_file() => {}
            "draft.txt" if kind.is_file() => {
                if !fs::read_to_string(item.path()).is_ok_and(|v| v.is_empty()) {
                    return false;
                }
            }
            "transcripts" | "context" | "logs"
                if kind.is_dir() && dir_has_no_files(&item.path()) => {}
            _ => return false,
        }
    }
    data.join("entries.jsonl").is_file()
        && data.join("id").is_file()
        && ["transcripts", "context", "logs"]
            .iter()
            .all(|name| data.join(name).is_dir())
}
fn copy_tree(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir(target).map_err(|e| e.to_string())?;
    for item in fs::read_dir(source).map_err(|e| e.to_string())? {
        let item = item.map_err(|e| e.to_string())?;
        let destination = target.join(item.file_name());
        let kind = item.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            copy_tree(&item.path(), &destination)?;
        } else if kind.is_file() {
            fs::copy(item.path(), destination).map_err(|e| e.to_string())?;
        } else {
            return Err("session contains an unsupported filesystem entry".into());
        }
    }
    Ok(())
}
fn relocate_contents(source: &Path, destination: &Path) -> Result<bool, String> {
    let source_notes = source.join("notes.md");
    let source_data = source.join(".data");
    let target_notes = destination.join("notes.md");
    let target_data = destination.join(".data");
    let source_assets = source.join("assets");
    let target_assets = destination.join("assets");
    let has_assets = source_assets.exists() || source_assets.is_symlink();
    if has_assets && !source_assets.is_dir() {
        return Err("session assets must be a directory".into());
    }
    if has_assets
        && source_assets
            .symlink_metadata()
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
    {
        return Err("session contains an unsupported filesystem entry".into());
    }
    match fs::rename(&source_notes, &target_notes) {
        Ok(()) => {
            if let Err(error) = fs::rename(&source_data, &target_data) {
                let _ = fs::rename(&target_notes, &source_notes);
                return Err(error.to_string());
            }
            if has_assets && let Err(error) = fs::rename(&source_assets, &target_assets) {
                let _ = fs::rename(&target_data, &source_data);
                let _ = fs::rename(&target_notes, &source_notes);
                return Err(error.to_string());
            }
            Ok(true)
        }
        Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {
            if let Err(error) = fs::copy(&source_notes, &target_notes) {
                let _ = fs::remove_file(&target_notes);
                return Err(error.to_string());
            }
            if let Err(error) = copy_tree(&source_data, &target_data) {
                let _ = fs::remove_file(&target_notes);
                let _ = fs::remove_dir_all(&target_data);
                return Err(error);
            }
            if has_assets && let Err(error) = copy_tree(&source_assets, &target_assets) {
                let _ = fs::remove_file(&target_notes);
                let _ = fs::remove_dir_all(&target_data);
                let _ = fs::remove_dir_all(&target_assets);
                return Err(error);
            }
            Ok(false)
        }
        Err(error) => Err(error.to_string()),
    }
}
fn usage() {
    println!(
        "usage: omatate <command> [args]\n\ncommands:\n  open                 choose a project, or reopen the active panel (default)\n  open-project <dir>   create or resume notes in a project directory\n  projects             list known project paths as JSON\n  create-project <parent> <name> <current>  internal: create and select a new folder\n  select-project <dir> <current>  internal: switch from the expected active project\n  start [name]         start a session (and open the panel)\n  resume <dir>         reopen an existing session and its draft\n  stop                 end the active session\n  toggle               open or close the panel; end the active session when closing\n  focus-toggle         enter or release panel keyboard focus\n  status               print the active session path, or inactive\n  section [title]      add a section to the active session\n  clip                 select and capture part of the screen\n  relocate <dir>       move the active session into <dir>\n  ai [on|off|toggle]   print or change the screenshot-analysis mode\n  ptt start|stop       push-to-talk press / release\n  render               regenerate notes.md\n  panel <cmd>          panel socket command"
    )
}

fn run(args: &[String]) -> Result<(), String> {
    let cmd = args.first().map(String::as_str).unwrap_or("");
    match (cmd, args.len()) {
        ("", 0) | ("open", 1) => launch_with_last_project("open"),
        ("help" | "-h" | "--help", 1) => {
            usage();
            Ok(())
        }
        ("projects", 1) => projects(),
        ("backend", 1) => backend::serve(),
        ("start", 1 | 2) => {
            let p = start(args.get(1).map(String::as_str))?;
            println!("{}", p.display());
            Ok(())
        }
        ("open-project", 2) => {
            let p = open_project(&args[1], None, None)
                .map_err(|e| format!("cannot open project {}: {e}", args[1]))?;
            launch_panel("ping")?;
            println!("{}", p.display());
            Ok(())
        }
        ("select-project", 3) => {
            let p = open_project(&args[1], Some(&args[2]), None)
                .map_err(|e| format!("cannot open project {}: {e}", args[1]))?;
            println!("{}", p.display());
            Ok(())
        }
        ("create-project", 4) => {
            let p = open_project(&args[1], Some(&args[3]), Some(&args[2]))
                .map_err(|e| format!("cannot open project {}: {e}", args[1]))?;
            println!("{}", p.display());
            Ok(())
        }
        ("resume", 2) => {
            let p = canonical(&args[1])
                .map_err(|e| format!("cannot resume session {}: {e}", args[1]))?;
            core::with_lock(|| {
                validate(&p)?;
                install_pointer(&p, None)?;
                remember(&p);
                Ok(())
            })
            .map_err(|e| format!("cannot resume session {}: {e}", p.display()))?;
            launch_panel("ping")?;
            println!("{}", p.display());
            Ok(())
        }
        ("stop", 1) => stop(),
        ("focus-toggle", 1) => launch_with_last_project("toggle-focus"),
        ("toggle", 1) => {
            if active(false).is_some() {
                stop()
            } else {
                match panel_request("quit", Duration::from_millis(1500)) {
                    Ok(_) => Ok(()),
                    Err(e @ PanelError::Rejected(_)) => Err(format!("cannot close panel: {e}")),
                    Err(_) => launch_with_last_project("open"),
                }
            }
        }
        ("status", 1) => {
            if let Some(p) = active(false) {
                println!("{}", p.display());
                Ok(())
            } else {
                println!("inactive");
                Err(String::new())
            }
        }
        ("render", 1) => core::with_lock(|| core::render(&active(true).unwrap())),
        ("clip", 1) => clip(),
        ("section", 1 | 2) => {
            let s = active(true).unwrap();
            let id = core::with_lock(|| {
                let entries = core::read_entries(&s)?;
                let id = entries
                    .iter()
                    .filter_map(|v| v.get("id").and_then(Value::as_i64))
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or("entry ids exhausted")?;
                let k = entries
                    .iter()
                    .filter(|v| v.get("kind").and_then(Value::as_str) == Some("section"))
                    .count()
                    + 1;
                core::rewrite_entry(
                    &s,
                    id,
                    json!({"id":id,"kind":"section","ts":core::timestamp(),"title":args.get(1).cloned().unwrap_or_else(||format!("Section {k}"))}),
                )?;
                Ok(id)
            })?;
            let _ = panel_request("reload", Duration::from_secs(1));
            println!("{id}");
            Ok(())
        }
        ("ai", 1 | 2)
            if args
                .get(1)
                .is_none_or(|v| matches!(v.as_str(), "on" | "off" | "toggle")) =>
        {
            let path = core::home().join(".config/omatate/ai");
            let mut state = if core::ai_enabled() { "on" } else { "off" };
            if let Some(a) = args.get(1) {
                state = match a.as_str() {
                    "toggle" => {
                        if state == "on" {
                            "off"
                        } else {
                            "on"
                        }
                    }
                    "on" => "on",
                    "off" => "off",
                    _ => unreachable!(),
                };
                fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
                core::atomic_write(&path, format!("{state}\n").as_bytes())?
            }
            println!("{state}");
            Ok(())
        }
        ("ptt", 2) if args[1] == "start" => ptt_start(),
        ("ptt", 2) if args[1] == "stop" => ptt_stop(),
        ("analyze", 2)
            if args[1].bytes().all(|b| b.is_ascii_digit())
                && args[1].parse::<i64>().is_ok_and(|n| n > 0) =>
        {
            analyze(args[1].parse().unwrap())
        }
        ("ingest", 2)
            if args[1].bytes().all(|b| b.is_ascii_digit())
                && args[1].parse::<i64>().is_ok_and(|n| n > 0) =>
        {
            ingest(args[1].parse().unwrap())
        }
        ("panel", 2)
            if matches!(
                args[1].as_str(),
                "hide" | "show" | "focus" | "toggle-focus" | "reload" | "quit" | "ping" | "restyle"
            ) =>
        {
            let v = panel_request(&args[1], Duration::from_millis(1500))
                .map_err(|e| format!("panel not running: {e}"))?;
            println!("{}", serde_json::to_string(&v).unwrap());
            Ok(())
        }
        ("cleanup-shot", 2) => {
            let p = PathBuf::from(&args[1]);
            let age = fs::metadata(&p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|m| SystemTime::now().duration_since(m).ok())
                .unwrap_or(Duration::from_secs(300));
            if age < Duration::from_secs(300) {
                thread::sleep(Duration::from_secs(300) - age)
            }
            let _ = fs::remove_file(&p);
            if let Some(parent) = p.parent() {
                let _ = fs::remove_dir(parent);
            }
            Ok(())
        }
        ("relocate", 2) => {
            let s = active(true).unwrap();
            let raw = expanded(&args[1]);
            if !raw.exists() {
                return Err(format!("destination does not exist: {}", raw.display()));
            }
            let d = raw.canonicalize().map_err(|e| e.to_string())?;
            if !d.is_dir() {
                return Err(format!("destination is not a directory: {}", d.display()));
            }
            if d.starts_with(s.canonicalize().map_err(|e| e.to_string())?) {
                return Err("destination must not be inside the current session".into());
            }
            for n in ["notes.md", ".data", "assets"] {
                if d.join(n).exists() || d.join(n).is_symlink() {
                    return Err(format!("destination already contains {n}"));
                }
            }
            core::with_lock(|| {
                if core::session_path().as_deref() != Some(s.as_path()) {
                    return Err("active project changed; reload projects and try again".into());
                }
                for n in ["notes.md", ".data", "assets"] {
                    if d.join(n).exists() || d.join(n).is_symlink() {
                        return Err(format!("destination already contains {n}"));
                    }
                }
                let result = (|| {
                    let renamed = relocate_contents(&s, &d)?;
                    if let Err(error) = core::atomic_write(
                        &core::runtime_dir().join("session"),
                        format!("{}\n", d.display()).as_bytes(),
                    ) {
                        if renamed {
                            if d.join("assets").exists() {
                                let _ = fs::rename(d.join("assets"), s.join("assets"));
                            }
                            let _ = fs::rename(d.join(".data"), s.join(".data"));
                            let _ = fs::rename(d.join("notes.md"), s.join("notes.md"));
                        } else {
                            let _ = fs::remove_file(d.join("notes.md"));
                            let _ = fs::remove_dir_all(d.join(".data"));
                            let _ = fs::remove_dir_all(d.join("assets"));
                        }
                        return Err(error);
                    }
                    Ok(renamed)
                })();
                let renamed = result?;
                if !renamed {
                    let _ = fs::remove_file(s.join("notes.md"));
                    let _ = fs::remove_dir_all(s.join(".data"));
                    let _ = fs::remove_dir_all(s.join("assets"));
                }
                let _ = fs::remove_dir(&s);
                remember(&d);
                Ok(())
            })?;
            println!("{}", d.display());
            Ok(())
        }
        _ => {
            eprintln!("omatate: invalid command: {}", args.join(" "));
            eprintln!("run 'omatate help'");
            process::exit(2)
        }
    }
}
fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.len() == 2
        && args[0] == "ptt"
        && matches!(args[1].as_str(), "start" | "stop")
        && core::session_path().is_none()
    {
        exec_voxtype(&args[1])
    }
    if let Err(e) = run(&args) {
        if !e.is_empty() {
            eprintln!("{e}")
        }
        process::exit(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_name_rejects_nul_and_paths() {
        for name in [
            "",
            " \t",
            ".",
            "..",
            "../escape",
            "nested/project",
            "nested\\project",
            "bad\0name",
            " leading",
            "trailing ",
        ] {
            assert!(!valid_project_name(name), "{name:?}");
        }
        assert!(valid_project_name("My new project"));
    }
}
