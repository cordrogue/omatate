use serde_json::Value;
use std::env;
use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

pub fn runtime_dir() -> PathBuf {
    match env::var_os("XDG_RUNTIME_DIR").filter(|base| !base.is_empty()) {
        Some(base) => PathBuf::from(base).join("ui-notes"),
        None => PathBuf::from(format!("/tmp/ui-notes-{}", unsafe { libc::getuid() })),
    }
}

pub fn session_path() -> Option<PathBuf> {
    let pinned = env::var_os("UI_NOTES_SESSION_PATH").map(PathBuf::from);
    let worker_id = env::var("UI_NOTES_SESSION_ID").ok();
    resolve_session(pinned, worker_id.as_deref(), pointer_path(), |path| {
        fs::read_to_string(path.join(".data/id")).ok()
    })
}

fn resolve_session(
    pinned: Option<PathBuf>,
    worker_id: Option<&str>,
    pointer: Option<PathBuf>,
    token_of: impl Fn(&Path) -> Option<String>,
) -> Option<PathBuf> {
    let Some(path) = pinned else {
        return pointer;
    };
    let Some(worker_id) = worker_id.filter(|id| !id.is_empty()) else {
        return Some(path);
    };
    if token_of(&path).is_some_and(|token| token.trim() == worker_id) {
        return Some(path);
    }
    pointer.filter(|path| token_of(path).is_some_and(|token| token.trim() == worker_id))
}

fn pointer_path() -> Option<PathBuf> {
    fs::read_to_string(runtime_dir().join("session"))
        .ok()
        .map(|s| s.trim_end_matches(['\n', '\r']).to_owned())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

pub fn read_entries(session: &Path) -> Result<Vec<Value>, String> {
    let path = session.join(".data/entries.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(|e| e.to_string()))
        .collect()
}

pub fn with_lock<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let runtime = runtime_dir();
    fs::create_dir_all(&runtime).map_err(|e| e.to_string())?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(runtime.join("lock"))
        .map_err(|e| e.to_string())?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let result = f();
    unsafe {
        libc::flock(lock.as_raw_fd(), libc::LOCK_UN);
    }
    result
}

pub fn write_entries(session: &Path, entries: &[Value]) -> Result<(), String> {
    let mut bytes = Vec::new();
    for entry in entries {
        serde_json::to_writer(&mut bytes, entry).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
    }
    atomic_write(&session.join(".data/entries.jsonl"), &bytes)
}

pub fn render(session: &Path) -> Result<(), String> {
    render_entries(session, &read_entries(session)?)
}

pub fn render_entries(session: &Path, entries: &[Value]) -> Result<(), String> {
    let name = session.file_name().and_then(|v| v.to_str()).unwrap_or("");
    let (label, date) = label_date(name);
    let mut out = format!("# UI review — {label} · {date}\n\n");
    let sections = entries
        .iter()
        .any(|v| v.get("kind").and_then(Value::as_str) == Some("section"));
    let mut sorted = entries.to_vec();
    sorted.sort_by_key(|v| v.get("id").and_then(Value::as_i64).unwrap_or(0));
    for entry in sorted {
        if entry.get("kind").and_then(Value::as_str) == Some("section") {
            out.push_str(&format!(
                "## {}\n\n",
                entry.get("title").and_then(Value::as_str).unwrap_or("")
            ));
            continue;
        }
        let transcript = entry
            .get("transcript")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let comment = if !transcript.is_empty() {
            transcript
        } else if matches!(
            entry.get("status").and_then(Value::as_str),
            Some("recording" | "transcribing")
        ) {
            "_transcribing…_"
        } else {
            "_no transcript_"
        };
        if let Some(asset) = entry
            .get("asset")
            .and_then(Value::as_str)
            .filter(|asset| !asset.is_empty())
        {
            out.push_str(&format!("![Screen capture]({asset})\n\n"));
        }
        if entry.get("ai").and_then(Value::as_bool) == Some(false) {
            out.push_str(comment);
            out.push_str("\n\n");
            continue;
        }
        let id = entry
            .get("id")
            .and_then(Value::as_i64)
            .ok_or("entry missing integer id")?;
        let context = entry
            .get("context")
            .and_then(Value::as_object)
            .filter(|value| !value.is_empty());
        let title = context
            .and_then(|c| c.get("title"))
            .and_then(Value::as_str)
            .filter(|title| !title.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Entry {id}"));
        out.push_str(&format!(
            "{} {id}. {title}\n",
            if sections { "###" } else { "##" }
        ));
        if let Some(summary) = context
            .and_then(|c| c.get("summary"))
            .and_then(Value::as_str)
        {
            out.push_str(summary);
        } else if entry.get("context_status").and_then(Value::as_str) == Some("error") {
            out.push_str("_analysis failed_");
        } else {
            out.push_str("_analysis pending…_");
        }
        out.push_str("\n\n> ");
        out.push_str(&comment.replace('\n', "\n> "));
        out.push_str("\n\n");
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    atomic_write(&session.join("notes.md"), out.as_bytes())
}

fn label_date(name: &str) -> (String, String) {
    let bytes = name.as_bytes();
    if bytes.len() >= 15
        && bytes.get(8) == Some(&b'-')
        && bytes[..8].iter().all(u8::is_ascii_digit)
        && bytes[9..15].iter().all(u8::is_ascii_digit)
        && (bytes.len() == 15 || bytes.get(15) == Some(&b'-'))
    {
        let label = if bytes.get(15) == Some(&b'-') && name.len() > 16 {
            name[16..].to_owned()
        } else {
            name.to_owned()
        };
        return (
            label,
            format!("{}-{}-{}", &name[..4], &name[4..6], &name[6..8]),
        );
    }
    (name.to_owned(), today())
}

fn time_format(format: &str) -> String {
    let mut now: libc::time_t = 0;
    unsafe {
        libc::time(&mut now);
    }
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::localtime_r(&now, &mut tm);
    }
    let fmt = CString::new(format).unwrap();
    let mut buf = [0u8; 64];
    let n = unsafe { libc::strftime(buf.as_mut_ptr().cast(), buf.len(), fmt.as_ptr(), &tm) };
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

fn today() -> String {
    time_format("%Y-%m-%d")
}
pub fn timestamp() -> String {
    time_format("%Y-%m-%dT%H:%M:%S")
}
pub fn filename_timestamp() -> String {
    time_format("%Y%m%d-%H%M%S")
}

pub fn ai_enabled() -> bool {
    fs::read_to_string(home().join(".config/ui-notes/ai"))
        .map(|s| s.trim() == "on")
        .unwrap_or(false)
}

pub fn home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("invalid file name")?;
    let tmp = path.with_file_name(format!("{name}.tmp"));
    let permissions = match fs::metadata(path) {
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.to_string()),
    };
    let mut file = File::create(&tmp).map_err(|e| e.to_string())?;
    let result = (|| {
        if let Some(permissions) = permissions {
            file.set_permissions(permissions)?;
        }
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(|e| e.to_string())
}

pub fn rewrite_entry(session: &Path, id: i64, replacement: Value) -> Result<(), String> {
    let mut entries = read_entries(session)?;
    let mut found = false;
    entries.retain(|item| {
        if item.get("id").and_then(Value::as_i64) == Some(id) {
            if !found {
                found = true;
                true
            } else {
                false
            }
        } else {
            true
        }
    });
    if let Some(pos) = entries
        .iter()
        .position(|v| v.get("id").and_then(Value::as_i64) == Some(id))
    {
        entries[pos] = replacement;
    } else {
        entries.push(replacement);
    }
    write_entries(session, &entries)?;
    render_entries(session, &entries)
}

pub fn update_entry(
    session: &Path,
    id: i64,
    change: impl FnOnce(&mut Value) -> Result<(), String>,
) -> Result<(), String> {
    let mut entries = read_entries(session)?;
    let pos = entries
        .iter()
        .position(|v| v.get("id").and_then(Value::as_i64) == Some(id))
        .ok_or_else(|| format!("entry {id} not found"))?;
    change(&mut entries[pos])?;
    let replacement = entries[pos].clone();
    entries.retain(|v| v.get("id").and_then(Value::as_i64) != Some(id));
    entries.push(replacement);
    write_entries(session, &entries)?;
    render_entries(session, &entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;

    #[test]
    fn session_keeps_matching_pinned_path() {
        let tokens = HashMap::from([(Path::new("/pinned"), " worker \n")]);
        assert_eq!(
            resolve_session(
                Some(PathBuf::from("/pinned")),
                Some("worker"),
                Some(PathBuf::from("/current")),
                |path| tokens.get(path).map(|token| token.to_string()),
            ),
            Some(PathBuf::from("/pinned"))
        );
    }

    #[test]
    fn session_follows_matching_pointer_after_relocation() {
        let tokens = HashMap::from([
            (Path::new("/pinned"), "other"),
            (Path::new("/current"), " worker\n"),
        ]);
        assert_eq!(
            resolve_session(
                Some(PathBuf::from("/pinned")),
                Some("worker"),
                Some(PathBuf::from("/current")),
                |path| tokens.get(path).map(|token| token.to_string()),
            ),
            Some(PathBuf::from("/current"))
        );
    }

    #[test]
    fn session_rejects_mismatched_or_missing_worker_tokens() {
        let tokens = HashMap::from([
            (Path::new("/pinned"), "other"),
            (Path::new("/current"), "another"),
        ]);
        for pinned in ["/pinned", "/missing"] {
            for pointer in ["/current", "/missing"] {
                assert_eq!(
                    resolve_session(
                        Some(PathBuf::from(pinned)),
                        Some("worker"),
                        Some(PathBuf::from(pointer)),
                        |path| tokens.get(path).map(|token| token.to_string()),
                    ),
                    None
                );
            }
        }
    }

    #[test]
    fn session_keeps_pinned_path_without_worker_id() {
        let tokens = HashMap::from([(Path::new("/current"), "other")]);
        for worker_id in [None, Some("")] {
            assert_eq!(
                resolve_session(
                    Some(PathBuf::from("/pinned")),
                    worker_id,
                    Some(PathBuf::from("/current")),
                    |path| tokens.get(path).map(|token| token.to_string()),
                ),
                Some(PathBuf::from("/pinned"))
            );
        }
    }

    #[test]
    fn atomic_write_removes_temp_file_after_rename_failure() {
        let root = env::temp_dir().join(format!("ui-notes-atomic-write-test-{}", unsafe {
            libc::getpid()
        }));
        let path = root.join("destination");
        fs::create_dir_all(&path).unwrap();
        assert!(atomic_write(&path, b"contents").is_err());
        assert!(!root.join("destination.tmp").exists());
        assert!(path.is_dir());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn locked_writers_preserve_each_others_records() {
        let root =
            env::temp_dir().join(format!("ui-notes-lock-test-{}", unsafe { libc::getpid() }));
        let _ = fs::remove_dir_all(&root);
        let session = root.join("session");
        fs::create_dir_all(session.join(".data")).unwrap();
        fs::write(session.join(".data/entries.jsonl"), "").unwrap();
        unsafe {
            env::set_var("XDG_RUNTIME_DIR", root.join("runtime"));
        }
        let session = Arc::new(session);
        let threads = (1..=12)
            .map(|id| {
                let session = Arc::clone(&session);
                std::thread::spawn(move || {
                    with_lock(|| {
                        let mut values = read_entries(&session)?;
                        values.push(json!({"id": id}));
                        write_entries(&session, &values)
                    })
                })
            })
            .collect::<Vec<_>>();
        for thread in threads {
            thread.join().unwrap().unwrap();
        }
        assert_eq!(read_entries(&session).unwrap().len(), 12);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn render_matches_contract_golden_output() {
        let root = env::temp_dir().join(format!("ui-notes-render-test-{}", unsafe {
            libc::getpid()
        }));
        let _ = fs::remove_dir_all(&root);
        let session = root.join("20260904-120000-checkout");
        fs::create_dir_all(session.join(".data")).unwrap();
        let entries = vec![
            json!({"id":1,"status":"recording","context":{}}),
            json!({"id":2,"kind":"section","title":"Checkout flow"}),
            json!({"id":3,"status":"done","context":{"title":"","summary":"Summary"},"transcript":"first\nsecond"}),
            json!({"id":4,"status":"done","context_status":"error","transcript":""}),
            json!({"id":5,"ai":false,"status":"done","transcript":" plain note \n next "}),
            json!({"id":6,"ai":false,"status":"transcribing"}),
            json!({"id":7,"ai":false,"status":"done","transcript":"","asset":"assets/clip.png"}),
        ];
        write_entries(&session, &entries).unwrap();
        render(&session).unwrap();
        assert_eq!(
            fs::read_to_string(session.join("notes.md")).unwrap(),
            "# UI review — checkout · 2026-09-04\n\n### 1. Entry 1\n_analysis pending…_\n\n> _transcribing…_\n\n## Checkout flow\n\n### 3. Entry 3\nSummary\n\n> first\n> second\n\n### 4. Entry 4\n_analysis failed_\n\n> _no transcript_\n\nplain note \n next\n\n_transcribing…_\n\n![Screen capture](assets/clip.png)\n\n_no transcript_\n"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn render_uses_filename_and_today_for_non_timestamp_session() {
        let root =
            env::temp_dir().join(format!("ui-notes-label-test-{}", unsafe { libc::getpid() }));
        let session = root.join("project-name");
        fs::create_dir_all(session.join(".data")).unwrap();
        fs::write(session.join(".data/entries.jsonl"), "").unwrap();
        render(&session).unwrap();
        assert_eq!(
            fs::read_to_string(session.join("notes.md")).unwrap(),
            format!("# UI review — project-name · {}\n", today())
        );
        let _ = fs::remove_dir_all(root);
    }
}
