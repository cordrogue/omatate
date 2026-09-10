//! The Quickshell frontend uses one JSON request and response per line.
use omatate::core;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, BufRead, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Polls compare a metadata stamp taken before the last full read, so a change
/// racing that read still forces the next full snapshot. Projects stay watched
/// once seen so a deleted and restored folder is noticed again.
#[derive(Default)]
struct SnapshotCache {
    stamp: String,
    projects: BTreeSet<String>,
}

fn metadata_stamp(path: &Path) -> String {
    fs::metadata(path)
        .map(|metadata| {
            format!(
                "{}:{}:{}:{}:{}",
                metadata.dev(),
                metadata.ino(),
                metadata.size(),
                metadata.mtime(),
                metadata.mtime_nsec()
            )
        })
        .unwrap_or_else(|_| "-".into())
}

fn snapshot_stamp(theme: &Value, projects: &BTreeSet<String>) -> String {
    let pointer = core::runtime_dir().join("session");
    let pointer_text = fs::read_to_string(&pointer).unwrap_or_default();
    let pointer_text = pointer_text.trim_end_matches(['\n', '\r']);
    let active = PathBuf::from(pointer_text);
    let mut parts = vec![metadata_stamp(&pointer), pointer_text.into()];
    for relative in [".data/entries.jsonl", ".data/draft.txt", ".data/id"] {
        parts.push(if active.as_os_str().is_empty() {
            "-".into()
        } else {
            metadata_stamp(&active.join(relative))
        });
    }
    for path in [
        core::home().join(".config/omatate/ai"),
        crate::keyboard::config_path(),
        crate::registry(),
        core::home().join("Documents/omatate"),
    ] {
        parts.push(metadata_stamp(&path));
    }
    for project in projects {
        let project = Path::new(project);
        if project == active {
            continue;
        }
        parts.push(metadata_stamp(&project.join(".data/entries.jsonl")));
        parts.push(metadata_stamp(&project.join(".data/id")));
    }
    parts.push(theme.to_string());
    serde_json::to_string(&parts).unwrap()
}

fn update_snapshot_cache(cache: &mut SnapshotCache, state: &Value, stamp: String) {
    cache.projects.extend(
        state["projects"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(String::from),
    );
    cache.stamp = stamp;
}

fn token(session: &Path) -> Result<String, String> {
    let token = fs::read_to_string(session.join(".data/id")).map_err(|e| e.to_string())?;
    let token = token.trim();
    if token.is_empty() {
        return Err("session has no identity token".into());
    }
    Ok(token.into())
}

fn optional_text(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error.to_string()),
    }
}

fn snapshot_entries(session: &Path, current: Option<Vec<Value>>) -> Result<Vec<Value>, String> {
    match current {
        Some(entries) => Ok(entries),
        None => core::read_entries(session),
    }
}

fn snapshot(theme: &Value, current_entries: Option<Vec<Value>>) -> Result<Value, String> {
    let session = core::session_path();
    let (identity, entries, draft) = match &session {
        Some(path) => (
            token(path)?,
            snapshot_entries(path, current_entries)?,
            optional_text(&path.join(".data/draft.txt"))?,
        ),
        None => (String::new(), vec![], String::new()),
    };
    let (keys, warning) = crate::keyboard::load();
    Ok(json!({
        "session": session.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
        "token": identity, "entries": entries, "draft": draft,
        "ai": core::ai_enabled(), "projects": crate::project_paths()?,
        "keys": keys, "keysWarning": warning, "runtimeDir": core::runtime_dir(),
        "theme": theme,
    }))
}

fn string<'a>(request: &'a Value, name: &str) -> Result<&'a str, String> {
    request
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string {name}"))
}

// Called only while holding the same lock used by project switches and workers.
fn expected_session(request: &Value) -> Result<PathBuf, String> {
    let expected = string(request, "session")?;
    let expected_token = string(request, "token")?;
    let session = core::session_path().ok_or("inactive")?;
    if expected.is_empty()
        || expected_token.is_empty()
        || session != Path::new(expected)
        || token(&session)? != expected_token
    {
        return Err("active project changed; reload before saving".into());
    }
    Ok(session)
}

fn entry_id(request: &Value) -> Result<i64, String> {
    request
        .get("entryId")
        .and_then(Value::as_i64)
        .filter(|id| *id > 0)
        .ok_or_else(|| "missing positive integer entryId".into())
}

#[derive(Default)]
struct MutationResult {
    warning: Option<String>,
    entries: Option<Vec<Value>>,
}

fn mutate(request: &Value, command: &str) -> Result<MutationResult, String> {
    let session = expected_session(request)?;
    if command == "draft" {
        return core::atomic_write(
            &session.join(".data/draft.txt"),
            string(request, "text")?.as_bytes(),
        )
        .map(|_| MutationResult::default());
    }
    let mut entries = core::read_entries(&session)?;
    match command {
        "note" => {
            let text = string(request, "text")?;
            if text.trim().is_empty() {
                return Err("note is empty".into());
            }
            let id = entries
                .iter()
                .filter_map(|e| e["id"].as_i64())
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("entry ids exhausted")?;
            entries.push(json!({"id":id,"ts":core::timestamp(),"ai":false,"status":"done","transcript":text}));
        }
        "edit" => {
            let id = entry_id(request)?;
            let text = string(request, "text")?;
            let entry = entries
                .iter_mut()
                .find(|e| e["id"].as_i64() == Some(id))
                .ok_or("entry disappeared")?;
            if matches!(entry["status"].as_str(), Some("recording" | "transcribing")) {
                return Err("finish transcription before editing this entry".into());
            }
            let key = if entry["kind"] == "section" {
                "title"
            } else {
                "transcript"
            };
            entry[key] = json!(text);
        }
        "delete-section" => {
            let id = entry_id(request)?;
            let entry = entries
                .iter()
                .find(|e| e["id"].as_i64() == Some(id))
                .ok_or("entry disappeared")?;
            if entry["kind"] != "section" {
                return Err("only section headings can be deleted".into());
            }
            entries.retain(|e| e["id"].as_i64() != Some(id));
        }
        "delete-note" => {
            let id = entry_id(request)?;
            let entry = entries
                .iter()
                .find(|e| e["id"].as_i64() == Some(id))
                .ok_or("entry disappeared")?;
            if entry["kind"] == "section" {
                return Err("section headings must be deleted with delete-section".into());
            }
            if matches!(entry["status"].as_str(), Some("recording" | "transcribing")) {
                return Err("finish transcription before deleting this entry".into());
            }
            if entry["context_status"] == "pending" {
                return Err("finish analysis before deleting this entry".into());
            }
            entries.retain(|e| e["id"].as_i64() != Some(id));
        }
        _ => return Err("unknown command".into()),
    }
    core::write_entries(&session, &entries)?;
    if command == "note" {
        // Keep the draft when saving fails, and avoid duplicate notes on retry.
        if let Err(error) = core::atomic_write(&session.join(".data/draft.txt"), b"") {
            entries.pop();
            core::write_entries(&session, &entries)
                .map_err(|rollback| format!("{error}; cannot roll back saved note: {rollback}"))?;
            return Err(error);
        }
    }
    // entries.jsonl is authoritative. A derived Markdown failure must not make a
    // successful note look unsaved and cause the frontend to insert it twice.
    Ok(MutationResult {
        warning: core::render_entries(&session, &entries)
            .err()
            .map(|error| format!("Entries saved but cannot render notes.md: {error}")),
        entries: Some(entries),
    })
}

fn handle(
    request: Value,
    themes: &mut crate::theme::ThemeCache,
    cache: &mut SnapshotCache,
) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    // Resolve subprocess-backed configuration before taking the storage lock.
    let theme = themes.current();
    let result = core::with_lock(|| {
        let command = string(&request, "cmd")?;
        if command == "snapshot" {
            let stamp = snapshot_stamp(theme, &cache.projects);
            if request.get("poll").and_then(Value::as_bool) == Some(true)
                && !cache.stamp.is_empty()
                && stamp == cache.stamp
            {
                return Ok(json!({"id":id,"ok":true,"unchanged":true}));
            }
            let state = snapshot(theme, None)?;
            update_snapshot_cache(cache, &state, stamp);
            return Ok(json!({"id":id,"ok":true,"state":state}));
        }
        if !matches!(
            command,
            "draft" | "note" | "edit" | "delete-section" | "delete-note"
        ) {
            return Err("unknown command".into());
        }
        let MutationResult {
            mut warning,
            entries,
        } = mutate(&request, command)?;
        let mut reply = json!({"id":id,"ok":true});
        let stamp = snapshot_stamp(theme, &cache.projects);
        match snapshot(theme, entries) {
            Ok(state) => {
                update_snapshot_cache(cache, &state, stamp);
                reply["state"] = state;
            }
            Err(error) => {
                let message = format!("Saved, but cannot refresh panel: {error}");
                warning = Some(warning.map_or(message.clone(), |old| format!("{old}; {message}")));
            }
        }
        if let Some(warning) = warning {
            reply["warning"] = json!(warning);
        }
        Ok(reply)
    });
    result.unwrap_or_else(|error| json!({"id":id,"ok":false,"error":error}))
}

pub fn serve() -> Result<(), String> {
    fs::create_dir_all(core::runtime_dir()).map_err(|e| e.to_string())?;
    let input = io::stdin();
    let mut output = io::stdout().lock();
    let mut themes = crate::theme::ThemeCache::default();
    let mut cache = SnapshotCache::default();
    for line in input.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let reply = match serde_json::from_str(&line) {
            Ok(request) => handle(request, &mut themes, &mut cache),
            Err(error) => json!({"id":null,"ok":false,"error":format!("invalid JSON: {error}")}),
        };
        writeln!(output, "{reply}").map_err(|e| e.to_string())?;
        output.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn snapshot_entries_reuses_successful_mutation_result() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let session = std::env::temp_dir().join(format!("omatate-backend-{nonce}"));
        fs::create_dir_all(session.join(".data")).unwrap();
        fs::write(session.join(".data/entries.jsonl"), "invalid JSON\n").unwrap();

        let current = vec![json!({"id": 1, "transcript": "saved"})];
        assert_eq!(
            snapshot_entries(&session, Some(current.clone())).unwrap(),
            current
        );
        assert!(snapshot_entries(&session, None).is_err());

        fs::remove_dir_all(session).unwrap();
    }
}
